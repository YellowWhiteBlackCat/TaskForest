//! The System-page dashboard's data layer (ARCH.md §8.1): the pure fold of
//! the shell projection into the summary card values. Kept in its own
//! non-render module so the paint file (`ui/system_dashboard.rs`) consumes
//! pre-folded strings and never reads observations inline.

use taskmanager_application::system_timeline::{TimelineMetric, TimelineSeries, TimelineStatistic};
use taskmanager_shell::SystemProjectionStore;
use taskmanager_shell::presentation::system_timeline::readout;

/// Pre-folded summary values for the segment's cards. `None` observations
/// fold to the shared dash string — never `0` / `0.0%`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DashboardSummaryModel {
    /// Latest global CPU utilization, already folded to a display string.
    pub cpu: String,
    /// Latest observed memory utilization, already folded to a display string.
    pub memory: String,
    /// Observed process count (`None` while no inventory has arrived).
    pub processes: Option<usize>,
    /// Live active-alert count from the shell's evaluation mirror.
    pub active_alerts: usize,
}

/// Fold the shell projection into the summary card values (pure).
pub(crate) fn summary_model(
    projection: &SystemProjectionStore,
    series: &TimelineSeries,
) -> DashboardSummaryModel {
    DashboardSummaryModel {
        cpu: readout(series, TimelineMetric::Cpu, TimelineStatistic::Latest),
        memory: readout(series, TimelineMetric::Memory, TimelineStatistic::Latest),
        processes: projection
            .processes
            .as_ref()
            .map(|processes| processes.len()),
        active_alerts: projection.alert_active.len(),
    }
}
