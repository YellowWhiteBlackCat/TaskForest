//! Adapt the authoritative in-memory rings to the application-owned read port.

use taskmanager_application::system_timeline::{
    MetricHistoryRevision, SystemHistoryWindow, TimelineHistorySource, TimelineMetric,
    TimelineSample, TimelineSeries, TimelineState, bytes_per_sec_to_mib,
};
use taskmanager_telemetry_store::{CorrelatedMetricHistory, CorrelatedSystemTelemetryHistory};

struct HistorySource<'a>(&'a CorrelatedSystemTelemetryHistory);

impl TimelineHistorySource for HistorySource<'_> {
    fn revision(&self, metric: TimelineMetric) -> MetricHistoryRevision {
        match metric {
            TimelineMetric::Cpu => revision(self.0.cpu_usage()),
            TimelineMetric::Memory => revision(self.0.memory_usage()),
            TimelineMetric::Disk => revision(self.0.storage_rate_total()),
            TimelineMetric::Network => revision(self.0.network_rate_total()),
        }
    }
    fn latest_completion_ms(&self, metric: TimelineMetric) -> Option<u64> {
        match metric {
            TimelineMetric::Cpu => self.0.cpu_usage().latest_completion_ms(),
            TimelineMetric::Memory => self.0.memory_usage().latest_completion_ms(),
            TimelineMetric::Disk => self.0.storage_rate_total().latest_completion_ms(),
            TimelineMetric::Network => self.0.network_rate_total().latest_completion_ms(),
        }
    }
    fn samples(&self, metric: TimelineMetric) -> Vec<TimelineSample> {
        match metric {
            TimelineMetric::Cpu => samples(self.0.cpu_usage(), |value| value),
            TimelineMetric::Memory => samples(self.0.memory_usage(), |value| value),
            TimelineMetric::Disk => samples(self.0.storage_rate_total(), bytes_per_sec_to_mib),
            TimelineMetric::Network => samples(self.0.network_rate_total(), bytes_per_sec_to_mib),
        }
    }
}
fn revision<T>(history: CorrelatedMetricHistory<T>) -> MetricHistoryRevision {
    let (len, last_revision) = history.watermark();
    MetricHistoryRevision {
        source_id: history.ring_id(),
        len,
        last_revision,
    }
}
fn samples<T: Clone>(
    history: CorrelatedMetricHistory<T>,
    value: impl Fn(T) -> f32,
) -> Vec<TimelineSample> {
    history
        .samples()
        .into_iter()
        .map(|sample| TimelineSample {
            completed_at_ms: sample.stamp.completed_at_ms(),
            measured_at_ms: sample.measured_at_ms,
            value: sample.value.map(&value),
        })
        .collect()
}

/// Select the inward read adapter; the application alone owns window and gap rules.
#[must_use]
pub fn project_timeline(
    state: &TimelineState,
    history: &CorrelatedSystemTelemetryHistory,
    window: SystemHistoryWindow,
) -> TimelineSeries {
    state.series(&HistorySource(history), window)
}

#[cfg(test)]
#[path = "../tests/headless/system_timeline.rs"]
mod tests;
