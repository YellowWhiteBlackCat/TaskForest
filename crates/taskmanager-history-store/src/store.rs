//! Bounded persistent-history orchestration.
//!
//! Admission/backpressure lives in [`pending`], JSONL and retention I/O in
//! [`retention_io`], single-writer ownership in [`lock`] and abandoned-
//! temporary hygiene in [`tmp_sweep`]. This module owns only store state,
//! flush coordination, the shared temporary-naming vocabulary and
//! diagnostics reconciliation.

mod lock;
mod pending;
mod retention_io;
mod tmp_sweep;

pub use pending::{
    MAX_PENDING_BYTES, MAX_PENDING_SAMPLES, MAX_PENDING_SERIES, MAX_SERIES_KEY_BYTES,
    MAX_TRACKED_SERIES, RecordSampleOutcome, RecordSampleRejection,
};
pub use retention_io::{
    MAX_DIRECTORY_ENTRIES_PER_SCAN, MAX_SERIES_FILE_BYTES, MAX_SERIES_FILES,
    MAX_SERIES_FILES_PER_SCAN,
};
pub use tmp_sweep::STALE_TEMPORARY_AGE_MS;

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use taskmanager_core::HistorySeriesKey;

pub use self::lock::{HistoryWriterClaimStatus, probe_root_lock};
use self::lock::{RootLockOwnership, acquire_root_lock};
use self::pending::{PendingSample, requeue_failed};
use crate::HistoryStoreError;
use crate::query::HistoryQuery;
use crate::retention::RetentionPolicy;

/// How often a flush also runs the TTL trim pass. Cardinality and byte quota
/// reconciliation run on every flush.
pub const TRIM_INTERVAL_MS: u64 = 10 * 60 * 1000;

const SERIES_EXTENSION: &str = "jsonl";

/// The persistent history store. Share it as `Arc<dyn HistoryRecordSink>` and
/// call [`Self::flush`] on a cadence owned by the composition edge.
pub struct PersistentHistoryStore {
    pub(super) root: PathBuf,
    pub(super) policy: RetentionPolicy,
    /// Injected liveness probe shared by claim takeover and the stale-
    /// temporary sweep: composition decides how a pid is proven gone.
    holder_is_gone: fn(u32) -> bool,
    pub(super) state: Mutex<StoreState>,
    /// Serializes flushes while keeping telemetry admission off the I/O lock.
    flush_lock: Mutex<()>,
    /// Dropped after the store's graceful flush; token matching prevents an
    /// obsolete instance from unlinking a replacement owner's claim.
    _lock: RootLockOwnership,
}

#[derive(Default)]
pub(super) struct StoreState {
    pending: HashMap<HistorySeriesKey, VecDeque<PendingSample>>,
    pending_samples: usize,
    pending_bytes: usize,
    next_enqueue_order: u64,
    /// Highest revision accepted per series in this process. A new OS session
    /// legitimately restarts revisions at one.
    last_recorded_revision: HashMap<HistorySeriesKey, u64>,
    /// Accepted keys that have not reached disk. Backpressure can release
    /// their guards when their final pending sample is dropped.
    unpersisted_series: HashSet<HistorySeriesKey>,
    last_seen_completed_ms: u64,
    last_trim_ms: Option<u64>,
    status: HistoryStoreStatus,
}

/// Diagnostics for surfaces that report store health. Counters saturate and
/// gauges describe the current bounded in-memory state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryStoreStatus {
    pub records_received: u64,
    pub duplicate_records_dropped: u64,
    pub samples_dropped_backpressure: u64,
    pub samples_rejected_resource_limit: u64,
    pub pending_series: usize,
    pub pending_samples: usize,
    pub pending_bytes: usize,
    pub tracked_series: usize,
    pub flushes: u64,
    pub samples_written: u64,
    pub corrupt_lines_skipped: u64,
}

/// What one flush actually did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlushReport {
    pub appended_series: usize,
    pub appended_samples: usize,
    pub ttl_trimmed_files: usize,
    pub quota_trimmed_files: usize,
    /// Stale temporaries of provably dead writers removed by this flush's
    /// hygiene pass. Removals are best-effort: failures are counted beside
    /// them and never fail the flush.
    pub stale_temporaries_swept: usize,
    pub temporary_sweep_failures: usize,
}

struct FlushCompletion<'a> {
    now_ms: u64,
    trim_due: bool,
    retention_succeeded: bool,
    samples_written: u64,
    corrupt_skipped: u64,
    persisted_series: &'a HashSet<HistorySeriesKey>,
    retired_series: &'a HashSet<HistorySeriesKey>,
}

impl PersistentHistoryStore {
    /// Open or create the directory and claim its injected-liveness
    /// single-writer lock. Existing series are reconciled on the first flush.
    pub fn open(
        root: impl Into<PathBuf>,
        policy: RetentionPolicy,
        holder_is_gone: fn(u32) -> bool,
    ) -> Result<Self, HistoryStoreError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|error| {
            HistoryStoreError::new(
                crate::HistoryStoreErrorKind::CreateDirectory,
                format!("{}: {error}", root.display()),
            )
        })?;
        let lock = acquire_root_lock(&root, holder_is_gone)?;
        Ok(Self {
            root,
            policy,
            holder_is_gone,
            state: Mutex::new(StoreState::default()),
            flush_lock: Mutex::new(()),
            _lock: lock,
        })
    }

    /// Read-only view over the same directory.
    #[must_use]
    pub fn query(&self) -> HistoryQuery {
        HistoryQuery::new(self.root.clone())
    }

    #[must_use]
    pub const fn policy(&self) -> RetentionPolicy {
        self.policy
    }

    #[must_use]
    pub fn status(&self) -> HistoryStoreStatus {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        HistoryStoreStatus {
            pending_series: state.pending.len(),
            pending_samples: state.pending_samples,
            pending_bytes: state.pending_bytes,
            tracked_series: state.last_recorded_revision.len(),
            ..state.status
        }
    }

    /// Flush bounded pending queues, then enforce on-disk cardinality and byte
    /// quota. TTL runs on its slower cadence; quota cannot drift between TTL
    /// passes. Failed appends return samples to the front of their queue and
    /// reapply global backpressure before this method returns.
    pub fn flush(&self, now_ms: u64) -> Result<FlushReport, HistoryStoreError> {
        let _flush_guard = self
            .flush_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (pending, trim_due) = self.take_pending(now_ms);
        let mut report = FlushReport::default();
        let mut failure = None;
        let mut samples_written = 0u64;
        let mut corrupt_skipped = 0u64;
        let mut persisted_series = HashSet::new();

        for (key, samples) in pending {
            match self.append_samples(&key, &samples) {
                Ok(corrupt) => {
                    report.appended_series = report.appended_series.saturating_add(1);
                    report.appended_samples = report.appended_samples.saturating_add(samples.len());
                    samples_written = samples_written
                        .saturating_add(u64::try_from(samples.len()).unwrap_or(u64::MAX));
                    corrupt_skipped =
                        corrupt_skipped.saturating_add(u64::try_from(corrupt).unwrap_or(u64::MAX));
                    persisted_series.insert(key);
                }
                Err(error) => {
                    let mut state = self
                        .state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    requeue_failed(&mut state, key, samples);
                    failure.get_or_insert(error);
                }
            }
        }

        // Hygiene before the retention scan: temporaries abandoned by dead
        // writers must not consume the directory-entry budget the scan
        // polices, or debris accumulation stalls persistence entirely.
        let sweep = tmp_sweep::sweep_stale_temporaries(&self.root, self.holder_is_gone);
        report.stale_temporaries_swept = sweep.stale_removed;
        report.temporary_sweep_failures = sweep.removal_failures;

        let mut retired_series = HashSet::new();
        let retention = self
            .apply_retention(now_ms, trim_due, &mut corrupt_skipped)
            .map(|(ttl_files, quota_files, retired)| {
                report.ttl_trimmed_files = ttl_files;
                report.quota_trimmed_files = quota_files;
                retired_series.extend(retired);
            });
        self.finish_flush(FlushCompletion {
            now_ms,
            trim_due,
            retention_succeeded: retention.is_ok(),
            samples_written,
            corrupt_skipped,
            persisted_series: &persisted_series,
            retired_series: &retired_series,
        });
        match failure.or_else(|| retention.err()) {
            Some(error) => Err(error),
            None => Ok(report),
        }
    }

    fn take_pending(
        &self,
        now_ms: u64,
    ) -> (HashMap<HistorySeriesKey, VecDeque<PendingSample>>, bool) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let pending = std::mem::take(&mut state.pending);
        state.pending_samples = 0;
        state.pending_bytes = 0;
        let trim_due = state
            .last_trim_ms
            .is_none_or(|last| now_ms.saturating_sub(last) >= TRIM_INTERVAL_MS);
        (pending, trim_due)
    }

    fn finish_flush(&self, completion: FlushCompletion<'_>) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.status.flushes = state.status.flushes.saturating_add(1);
        state.status.samples_written = state
            .status
            .samples_written
            .saturating_add(completion.samples_written);
        state.status.corrupt_lines_skipped = state
            .status
            .corrupt_lines_skipped
            .saturating_add(completion.corrupt_skipped);
        for key in completion.persisted_series {
            state.unpersisted_series.remove(key);
        }
        if !completion.retention_succeeded {
            return;
        }
        if completion.trim_due {
            state.last_trim_ms = Some(completion.now_ms);
        }
        for key in completion.retired_series {
            if state.pending.contains_key(key) {
                state.unpersisted_series.insert(key.clone());
            } else {
                state.last_recorded_revision.remove(key);
                state.unpersisted_series.remove(key);
            }
        }
    }
}

/// Filename marker carried by every temporary this crate creates
/// (`<final-extension>.tmp<pid>-<seq>`). The sweep matches exactly this
/// marker, so creation and cleanup cannot drift apart.
pub(crate) const TEMPORARY_FILE_MARKER: &str = "tmp";

/// Extension of the temporary sibling used while atomically replacing a
/// file whose final extension is `final_extension`.
pub(crate) fn temporary_extension(final_extension: &str) -> String {
    format!(
        "{final_extension}.{TEMPORARY_FILE_MARKER}{}",
        temporary_suffix()
    )
}

/// The writer pid embedded in one of this crate's temporary names. The full
/// `<pid>-<seq>` shape is validated so near-miss foreign names — including
/// the lock, claim, series and boot files, which never carry the marker —
/// parse to `None` and stay untouched by the sweep.
pub(crate) fn temporary_writer_pid(file_name: &str) -> Option<u32> {
    let (_, tail) = file_name.rsplit_once('.')?;
    let (pid, sequence) = tail.strip_prefix(TEMPORARY_FILE_MARKER)?.split_once('-')?;
    let pid: u32 = pid.parse().ok()?;
    sequence.parse::<u64>().ok()?;
    Some(pid)
}

/// Monotonic suffix prevents temporary-name collisions across rewrites.
pub(crate) fn temporary_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

impl Drop for PersistentHistoryStore {
    fn drop(&mut self) {
        let now_ms = self
            .state
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .last_seen_completed_ms;
        if now_ms > 0 {
            let _ = self.flush(now_ms);
        }
    }
}
