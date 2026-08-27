//! Bounded in-memory admission and backpressure for persistence samples.

use std::collections::VecDeque;

use taskmanager_core::{HistoricalSample, HistoryRecordSink, HistorySeriesKey};

use super::{PersistentHistoryStore, StoreState};
use crate::records::encode_line;

const MAX_PENDING_PER_SERIES: usize = 20_000;

/// Maximum number of distinct series buffered between flushes.
pub const MAX_PENDING_SERIES: usize = 1_024;
/// Maximum number of samples buffered across every series.
pub const MAX_PENDING_SAMPLES: usize = 8_192;
/// Maximum serialized payload plus series-key bytes buffered across every
/// series. Map/vector bookkeeping is additionally bounded by the cardinality
/// constants above.
pub const MAX_PENDING_BYTES: usize = 1024 * 1024;
/// Maximum number of in-session revision guards. A new identity is rejected
/// once this is reached until retention retires an old series.
pub const MAX_TRACKED_SERIES: usize = 1_024;
/// Maximum encoded file-stem size admitted from a series identity.
pub const MAX_SERIES_KEY_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy)]
pub(super) struct PendingSample {
    pub(super) sample: HistoricalSample,
    enqueue_order: u64,
    encoded_bytes: usize,
}

/// Why a sample could not enter the store's bounded in-memory state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordSampleRejection {
    InvalidSeriesScope,
    SeriesKeyTooLong {
        encoded_bytes: usize,
        max_bytes: usize,
    },
    TrackedSeriesLimit {
        max_series: usize,
    },
    PendingSeriesLimit {
        max_series: usize,
    },
    SampleTooLarge {
        encoded_bytes: usize,
        max_bytes: usize,
    },
}

/// Typed admission result. The `HistoryRecordSink` compatibility port cannot
/// return it, but callers that need direct admission feedback can use
/// [`PersistentHistoryStore::try_record_sample`]; both paths update status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordSampleOutcome {
    Accepted,
    AcceptedWithBackpressure { dropped_samples: usize },
    DuplicateRevision,
    Rejected(RecordSampleRejection),
}

impl PersistentHistoryStore {
    /// Admit one sample into the bounded in-memory write buffer.
    ///
    /// The trait-object sink delegates here but cannot return an outcome by
    /// contract; direct callers can distinguish duplicates, bounded loss and
    /// hard resource-limit rejection without parsing diagnostics text.
    pub fn try_record_sample(
        &self,
        key: HistorySeriesKey,
        sample: HistoricalSample,
    ) -> RecordSampleOutcome {
        let key_bytes = key.file_stem().len();
        let sample_bytes = encoded_sample_bytes(&sample);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.status.records_received = state.status.records_received.saturating_add(1);
        let rejection = admission_rejection(&state, &key, key_bytes, sample_bytes);
        if let Some(rejection) = rejection {
            state.status.samples_rejected_resource_limit = state
                .status
                .samples_rejected_resource_limit
                .saturating_add(1);
            return RecordSampleOutcome::Rejected(rejection);
        }
        if state
            .last_recorded_revision
            .get(&key)
            .is_some_and(|last| sample.revision <= *last)
        {
            state.status.duplicate_records_dropped =
                state.status.duplicate_records_dropped.saturating_add(1);
            return RecordSampleOutcome::DuplicateRevision;
        }

        state.last_seen_completed_ms = state.last_seen_completed_ms.max(sample.completed_at_ms);
        let is_new_series = !state.last_recorded_revision.contains_key(&key);
        state
            .last_recorded_revision
            .insert(key.clone(), sample.revision);
        if is_new_series {
            state.unpersisted_series.insert(key.clone());
        }
        let enqueue_order = state.next_enqueue_order;
        state.next_enqueue_order = state.next_enqueue_order.saturating_add(1);
        if !state.pending.contains_key(&key) {
            state.pending_bytes = state.pending_bytes.saturating_add(key_bytes);
        }
        state
            .pending
            .entry(key.clone())
            .or_default()
            .push_back(PendingSample {
                sample,
                enqueue_order,
                encoded_bytes: sample_bytes,
            });
        state.pending_samples = state.pending_samples.saturating_add(1);
        state.pending_bytes = state.pending_bytes.saturating_add(sample_bytes);

        let mut dropped = 0usize;
        while state
            .pending
            .get(&key)
            .is_some_and(|queue| queue.len() > MAX_PENDING_PER_SERIES)
        {
            if drop_pending_front(&mut state, &key) {
                dropped = dropped.saturating_add(1);
            } else {
                break;
            }
        }
        dropped = dropped.saturating_add(enforce_global_pending_limits(&mut state));
        state.status.samples_dropped_backpressure = state
            .status
            .samples_dropped_backpressure
            .saturating_add(u64::try_from(dropped).unwrap_or(u64::MAX));
        if dropped == 0 {
            RecordSampleOutcome::Accepted
        } else {
            RecordSampleOutcome::AcceptedWithBackpressure {
                dropped_samples: dropped,
            }
        }
    }
}

fn admission_rejection(
    state: &StoreState,
    key: &HistorySeriesKey,
    key_bytes: usize,
    sample_bytes: usize,
) -> Option<RecordSampleRejection> {
    if !key.is_valid() {
        Some(RecordSampleRejection::InvalidSeriesScope)
    } else if key_bytes > MAX_SERIES_KEY_BYTES {
        Some(RecordSampleRejection::SeriesKeyTooLong {
            encoded_bytes: key_bytes,
            max_bytes: MAX_SERIES_KEY_BYTES,
        })
    } else if sample_bytes > MAX_PENDING_BYTES {
        Some(RecordSampleRejection::SampleTooLarge {
            encoded_bytes: sample_bytes,
            max_bytes: MAX_PENDING_BYTES,
        })
    } else if !state.last_recorded_revision.contains_key(key)
        && state.last_recorded_revision.len() >= MAX_TRACKED_SERIES
    {
        Some(RecordSampleRejection::TrackedSeriesLimit {
            max_series: MAX_TRACKED_SERIES,
        })
    } else if !state.pending.contains_key(key) && state.pending.len() >= MAX_PENDING_SERIES {
        Some(RecordSampleRejection::PendingSeriesLimit {
            max_series: MAX_PENDING_SERIES,
        })
    } else {
        None
    }
}

fn encoded_sample_bytes(sample: &HistoricalSample) -> usize {
    encode_line(sample).len().saturating_add(1)
}

fn drop_pending_front(state: &mut StoreState, key: &HistorySeriesKey) -> bool {
    let (removed, became_empty) = match state.pending.get_mut(key) {
        Some(queue) => {
            let removed = queue.pop_front();
            (removed, queue.is_empty())
        }
        None => (None, false),
    };
    let Some(removed) = removed else {
        return false;
    };
    state.pending_samples = state.pending_samples.saturating_sub(1);
    state.pending_bytes = state.pending_bytes.saturating_sub(removed.encoded_bytes);
    if became_empty {
        state.pending.remove(key);
        state.pending_bytes = state.pending_bytes.saturating_sub(key.file_stem().len());
        if state.unpersisted_series.remove(key) {
            state.last_recorded_revision.remove(key);
        }
    }
    true
}

fn enforce_global_pending_limits(state: &mut StoreState) -> usize {
    let mut dropped = 0usize;
    while state.pending.len() > MAX_PENDING_SERIES
        || state.pending_samples > MAX_PENDING_SAMPLES
        || state.pending_bytes > MAX_PENDING_BYTES
    {
        let oldest = state
            .pending
            .iter()
            .filter_map(|(key, queue)| {
                queue
                    .front()
                    .map(|sample| (sample.enqueue_order, key.clone()))
            })
            .min_by_key(|(order, _)| *order)
            .map(|(_, key)| key);
        let Some(oldest) = oldest else {
            break;
        };
        if !drop_pending_front(state, &oldest) {
            break;
        }
        dropped = dropped.saturating_add(1);
    }
    dropped
}

pub(super) fn requeue_failed(
    state: &mut StoreState,
    key: HistorySeriesKey,
    mut failed: VecDeque<PendingSample>,
) {
    if failed.is_empty() {
        return;
    }
    let failed_count = failed.len();
    let failed_bytes = failed
        .iter()
        .map(|sample| sample.encoded_bytes)
        .fold(0usize, usize::saturating_add);
    let was_present = state.pending.contains_key(&key);
    let mut newer = state.pending.remove(&key).unwrap_or_default();
    failed.append(&mut newer);
    state.pending_samples = state.pending_samples.saturating_add(failed_count);
    state.pending_bytes = state.pending_bytes.saturating_add(failed_bytes);
    if !was_present {
        state.pending_bytes = state.pending_bytes.saturating_add(key.file_stem().len());
    }
    state.pending.insert(key, failed);
    let dropped = enforce_global_pending_limits(state);
    state.status.samples_dropped_backpressure = state
        .status
        .samples_dropped_backpressure
        .saturating_add(u64::try_from(dropped).unwrap_or(u64::MAX));
}

impl HistoryRecordSink for PersistentHistoryStore {
    fn record_sample(&self, key: HistorySeriesKey, sample: HistoricalSample) {
        let _ = self.try_record_sample(key, sample);
    }
}
