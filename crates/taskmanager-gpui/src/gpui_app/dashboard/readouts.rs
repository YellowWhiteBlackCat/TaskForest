//! Pure data-layer readout folds for the Dashboard page (ARCH.md §8.1): the
//! typed observation read behind the summary cards lives here so the render
//! module only paints the folded readout string.

use taskforest_core::core::metrics::CpuMetrics;
use taskmanager_shell::presentation::observed_percentage;

/// CPU summary-card readout: the shared whole-percent spelling for a current
/// observation, the shared missing-value dash when the provider has none.
pub(super) fn cpu_summary_readout(cpu: &CpuMetrics) -> String {
    observed_percentage(cpu.current_global_usage_pct())
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_dashboard_readouts_tests.rs"]
mod tests;
