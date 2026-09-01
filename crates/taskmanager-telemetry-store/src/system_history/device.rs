//! Per-device metric ingestion shared by storage, network, and GPU histories.
//!
//! Histories are keyed by `DeviceId`, reset on lifecycle generation advance,
//! gap-filled for absent or unavailable devices, and pruned of unknown devices.
//! Every sample a ring accepts is mirrored to the optional persistence fan-out
//! ([`PersistFanout`]), so the on-disk history always agrees with the
//! lifecycle decision the rings made — never with the raw provider input.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use taskmanager_core::{
    DeviceId, DeviceLifecycle, DevicePresence, HistoricalSample, HistoryMetric, HistoryRecordSink,
    HistorySeriesKey, SystemObservationState,
};

use super::{CorrelatedIngestionReport, CorrelatedTelemetryStamp, DeviceMetricHistory};

pub(super) struct DeviceMetricInput<T> {
    pub(super) device_id: DeviceId,
    pub(super) generation: u64,
    pub(super) value: Option<T>,
    /// Sampling time for this scalar, which may differ from its outer domain
    /// tick (SMART is collected on an independent cadence).
    pub(super) measured_at_ms: Option<u64>,
    pub(super) freshness: DeviceMeasurementFreshness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeviceMeasurementFreshness {
    /// Each accepted domain tick is a new observation for this scalar family.
    DomainTick,
    /// Only a strictly newer scalar sampling time may add a measured point.
    DistinctTimestamp,
}

/// Persisted-only scalar projections of one ring value: the series identity
/// plus its extractor. `fn` pointers (not closures) keep the fan-out a plain
/// static table, so the composite GPU point can derive its scalar series from
/// the same lifecycle walk that accepted the point itself.
pub(super) type PersistedScalars<T> = &'static [(HistoryMetric, fn(&T) -> Option<f64>)];

/// Optional persistence mirror for one device-history family. `None` sink
/// (history persistence disabled, or a ring intentionally not persisted — the
/// per-engine rows) makes [`Self::emit`] a no-op with no per-sample cost.
pub(super) struct PersistFanout<'a, T: 'static> {
    sink: Option<&'a dyn HistoryRecordSink>,
    scalars: PersistedScalars<T>,
}

impl<'a, T: 'static> PersistFanout<'a, T> {
    pub(super) const fn disabled() -> Self {
        Self {
            sink: None,
            scalars: &[],
        }
    }

    /// Attach the sink when persistence is enabled, stay inert otherwise.
    pub(super) const fn maybe(
        sink: Option<&'a dyn HistoryRecordSink>,
        scalars: PersistedScalars<T>,
    ) -> Self {
        Self { sink, scalars }
    }

    /// Mirror one accepted ring sample (or gap) into every persisted scalar
    /// series of this family. A `None` value emits explicit gaps — never a
    /// fabricated zero — and a `Some` composite keeps per-field availability.
    pub(super) fn emit(
        &self,
        device_id: &str,
        stamp: CorrelatedTelemetryStamp,
        measured_at_ms: Option<u64>,
        value: Option<&T>,
    ) {
        let Some(sink) = self.sink else {
            return;
        };
        for (metric, extract) in self.scalars.iter().copied() {
            sink.record_sample(
                HistorySeriesKey::for_device(metric, DeviceId::new(device_id.to_owned())),
                HistoricalSample {
                    revision: stamp.revision(),
                    completed_at_ms: stamp.completed_at_ms(),
                    measured_at_ms,
                    value: value.and_then(extract),
                },
            );
        }
    }
}

#[derive(Clone)]
struct DeviceIngestContext {
    capacity: usize,
    stamp: CorrelatedTelemetryStamp,
    measured_at_ms: Option<u64>,
    commit_gate: Arc<Mutex<()>>,
}

/// One complete accepted-device ingestion transaction. Named fields prevent
/// storage/network/GPU callers from swapping the lifecycle, timing, history,
/// or persistence inputs while keeping the commit atomic.
pub(super) struct DeviceMetricIngest<'a, 'p, T: 'static> {
    pub histories: &'a Mutex<HashMap<DeviceId, DeviceMetricHistory<T>>>,
    pub capacity: usize,
    pub commit_gate: Arc<Mutex<()>>,
    pub stamp: CorrelatedTelemetryStamp,
    pub measured_at_ms: Option<u64>,
    pub state: SystemObservationState,
    pub lifecycles: &'a BTreeMap<DeviceId, DeviceLifecycle>,
    pub inputs: Vec<DeviceMetricInput<T>>,
    pub persist: &'a PersistFanout<'p, T>,
}

pub(super) fn ingest_device_metrics<T>(
    transaction: DeviceMetricIngest<'_, '_, T>,
) -> CorrelatedIngestionReport
where
    T: Clone + 'static,
{
    let DeviceMetricIngest {
        histories,
        capacity,
        commit_gate,
        stamp,
        measured_at_ms,
        state,
        lifecycles,
        inputs,
        persist,
    } = transaction;
    let mut histories = histories
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let context = DeviceIngestContext {
        capacity,
        stamp,
        measured_at_ms,
        commit_gate,
    };
    if !state.is_current() {
        for (device_id, history) in histories.iter() {
            history
                .metric
                .push(context.stamp, context.measured_at_ms, None);
            persist.emit(
                device_id.as_str(),
                context.stamp,
                context.measured_at_ms,
                None,
            );
        }
        return CorrelatedIngestionReport::default();
    }

    let (mut inputs, duplicate_count) = unique_inputs(inputs);
    let mut report = CorrelatedIngestionReport {
        rejected_device_values: duplicate_count,
        ..CorrelatedIngestionReport::default()
    };

    for (device_id, lifecycle) in lifecycles {
        let input = inputs.remove(device_id.as_str()).flatten();
        match lifecycle.presence {
            DevicePresence::Present => ingest_present(
                &mut histories,
                context.clone(),
                device_id.as_str(),
                *lifecycle,
                input,
                &mut report,
                persist,
            ),
            DevicePresence::Absent | DevicePresence::Unavailable => {
                if input.is_some() {
                    report.rejected_device_values = report.rejected_device_values.saturating_add(1);
                }
                if let Some(history) = histories.get(device_id.as_str()) {
                    history.metric.push(stamp, measured_at_ms, None);
                    persist.emit(device_id.as_str(), stamp, measured_at_ms, None);
                }
            }
        }
    }
    report.rejected_device_values = report
        .rejected_device_values
        .saturating_add(inputs.values().filter(|input| input.is_some()).count());

    if matches!(state, SystemObservationState::Current { .. }) {
        let known: HashSet<&DeviceId> = lifecycles.keys().collect();
        let before = histories.len();
        histories.retain(|device_id, _| known.contains(device_id));
        report.pruned_device_histories = before.saturating_sub(histories.len());
    }
    report
}

fn ingest_present<T>(
    histories: &mut HashMap<DeviceId, DeviceMetricHistory<T>>,
    context: DeviceIngestContext,
    device_id: &str,
    lifecycle: DeviceLifecycle,
    input: Option<DeviceMetricInput<T>>,
    report: &mut CorrelatedIngestionReport,
    persist: &PersistFanout<'_, T>,
) where
    T: Clone,
{
    let supplied_input = input.is_some();
    let valid_input = input.filter(|input| {
        !device_id.is_empty()
            && lifecycle.generation.is_valid()
            && input.generation == lifecycle.generation.get()
    });
    let Some(input) = valid_input else {
        if supplied_input {
            report.rejected_device_values = report.rejected_device_values.saturating_add(1);
        }
        if let Some(history) = histories.get(device_id) {
            history
                .metric
                .push(context.stamp, context.measured_at_ms, None);
            persist.emit(device_id, context.stamp, context.measured_at_ms, None);
        }
        return;
    };
    match histories.get(device_id) {
        Some(history) if history.generation > lifecycle.generation.get() => {
            history
                .metric
                .push(context.stamp, context.measured_at_ms, None);
            persist.emit(device_id, context.stamp, context.measured_at_ms, None);
            report.rejected_device_values = report.rejected_device_values.saturating_add(1);
            return;
        }
        Some(history) if history.generation < lifecycle.generation.get() => {
            histories.insert(
                DeviceId::new(device_id),
                DeviceMetricHistory::new(
                    lifecycle.generation.get(),
                    context.capacity,
                    context.commit_gate.clone(),
                ),
            );
            report.reset_device_histories = report.reset_device_histories.saturating_add(1);
        }
        Some(_) => {}
        None => {
            histories.insert(
                DeviceId::new(device_id),
                DeviceMetricHistory::new(
                    lifecycle.generation.get(),
                    context.capacity,
                    context.commit_gate.clone(),
                ),
            );
        }
    }
    if let Some(history) = histories.get(device_id) {
        let mut measured_at_ms = input.measured_at_ms;
        let mut value = input.value.clone();
        if input.freshness == DeviceMeasurementFreshness::DistinctTimestamp
            && measured_at_ms.is_some_and(|candidate| {
                history
                    .metric
                    .latest_measured_at_in_transaction()
                    .is_some_and(|previous| candidate <= previous)
            })
        {
            measured_at_ms = None;
            value = None;
        }
        history
            .metric
            .push(context.stamp, measured_at_ms, value.clone());
        persist.emit(device_id, context.stamp, measured_at_ms, value.as_ref());
    }
}

fn unique_inputs<T>(
    inputs: Vec<DeviceMetricInput<T>>,
) -> (HashMap<DeviceId, Option<DeviceMetricInput<T>>>, usize) {
    let mut unique = HashMap::new();
    let mut duplicate_count = 0usize;
    for input in inputs {
        match unique.entry(input.device_id.clone()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Some(input));
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                if entry.get().is_some() {
                    duplicate_count = duplicate_count.saturating_add(2);
                } else {
                    duplicate_count = duplicate_count.saturating_add(1);
                }
                entry.insert(None);
            }
        }
    }
    (unique, duplicate_count)
}
