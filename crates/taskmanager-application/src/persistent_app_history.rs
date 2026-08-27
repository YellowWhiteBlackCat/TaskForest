//! Persistent per-application history projection.
//!
//! This module is the sole bridge from an accepted process-list snapshot to
//! the scalar persistence port. It owns identity provenance, deterministic
//! bounded admission and metric fan-out; frontends must not re-derive those
//! rules.

use std::sync::Arc;

use taskmanager_core::{
    ApplicationHistoryIdentity, HistoricalSample, HistoryMetric, HistoryRecordSink,
    HistorySeriesKey,
};

use crate::{AppGroup, ProcessItem, aggregate_apps};

/// Leaves room inside the history store's global series ceiling for system,
/// device and per-core facts. Each admitted application currently owns three
/// scalar series.
pub const MAX_PERSISTED_APPLICATION_IDENTITIES: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PersistentApplicationRecordReport {
    pub observed_applications: usize,
    pub recorded_applications: usize,
    pub rejected_identity_capacity: usize,
}

/// Single-threaded application-history fan-out owned by the active frontend
/// session. The sink remains the only persistence capability.
#[derive(Clone)]
pub struct PersistentApplicationHistoryRecorder {
    sink: Arc<dyn HistoryRecordSink>,
}

impl std::fmt::Debug for PersistentApplicationHistoryRecorder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PersistentApplicationHistoryRecorder")
            .finish_non_exhaustive()
    }
}

impl PersistentApplicationHistoryRecorder {
    #[must_use]
    pub fn new(sink: Arc<dyn HistoryRecordSink>) -> Self {
        Self { sink }
    }

    /// Fan one accepted process snapshot into stable application series.
    /// Runtime `EventSequence` is the revision authority and the correlated
    /// observation stamp is retained without consulting a host clock here.
    pub fn record_process_snapshot(
        &mut self,
        processes: &[ProcessItem],
        revision: u64,
        observed_at_ms: u64,
    ) -> PersistentApplicationRecordReport {
        let groups = aggregate(processes);
        if revision == 0 {
            return PersistentApplicationRecordReport {
                observed_applications: groups.len(),
                ..PersistentApplicationRecordReport::default()
            };
        }

        let mut identified = groups
            .iter()
            .filter_map(|group| history_identity(group).map(|identity| (identity, group)))
            .collect::<Vec<_>>();
        identified
            .sort_by(|(left, _), (right, _)| identity_order(left).cmp(&identity_order(right)));

        let mut recorded = 0usize;
        let rejected = identified
            .len()
            .saturating_sub(MAX_PERSISTED_APPLICATION_IDENTITIES);
        for (identity, group) in identified
            .into_iter()
            .take(MAX_PERSISTED_APPLICATION_IDENTITIES)
        {
            let sample = |value| HistoricalSample {
                revision,
                completed_at_ms: observed_at_ms,
                measured_at_ms: Some(observed_at_ms),
                value: Some(value),
            };
            self.sink.record_sample(
                HistorySeriesKey::for_application(
                    HistoryMetric::ApplicationCpuUsagePct,
                    identity.clone(),
                ),
                sample(f64::from(group.total_cpu_usage)),
            );
            self.sink.record_sample(
                HistorySeriesKey::for_application(
                    HistoryMetric::ApplicationMemoryBytes,
                    identity.clone(),
                ),
                sample(group.total_memory_bytes as f64),
            );
            self.sink.record_sample(
                HistorySeriesKey::for_application(HistoryMetric::ApplicationProcessCount, identity),
                sample(group.process_count as f64),
            );
            recorded = recorded.saturating_add(1);
        }

        PersistentApplicationRecordReport {
            observed_applications: groups.len(),
            recorded_applications: recorded,
            rejected_identity_capacity: rejected,
        }
    }
}

fn aggregate(processes: &[ProcessItem]) -> Vec<AppGroup> {
    let refs = processes.iter().collect::<Vec<_>>();
    aggregate_apps(&refs)
}

fn history_identity(group: &AppGroup) -> Option<ApplicationHistoryIdentity> {
    match group.application_identity.as_ref() {
        Some(identity) => {
            ApplicationHistoryIdentity::verified_launcher(identity.launcher_id.clone())
        }
        None => ApplicationHistoryIdentity::unverified_process_name(group.name.clone()),
    }
}

fn identity_order(identity: &ApplicationHistoryIdentity) -> (u8, &str) {
    (if identity.is_verified() { 0 } else { 1 }, identity.value())
}

#[cfg(test)]
#[path = "../tests/headless/application_persistent_app_history_tests.rs"]
mod tests;
