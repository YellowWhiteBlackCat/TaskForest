//! The System-page dashboard's data layer (ARCH.md §8.1): the pure fold of
//! the shell projection into the summary card values. Kept in its own
//! non-render module so the paint file (`ui/system_dashboard.rs`) consumes
//! pre-folded strings and never reads observations inline.

use taskmanager_shell::SystemProjectionStore;

use taskmanager_shell::presentation::missing_value;

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
pub(crate) fn summary_model(projection: &SystemProjectionStore) -> DashboardSummaryModel {
    let snapshot = projection.snapshot.as_ref();
    DashboardSummaryModel {
        cpu: snapshot
            .and_then(|snapshot| snapshot.cpu.current_global_usage_pct())
            .map_or_else(
                || missing_value().to_owned(),
                |value| format!("{value:.1}%"),
            ),
        memory: snapshot
            .and_then(|snapshot| snapshot.memory.used_percentage_observed())
            .map_or_else(
                || missing_value().to_owned(),
                |value| format!("{value:.1}%"),
            ),
        processes: projection
            .processes
            .as_ref()
            .map(|processes| processes.len()),
        active_alerts: projection.alert_active.len(),
    }
}
