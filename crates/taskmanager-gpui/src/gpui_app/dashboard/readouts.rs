//! Pure data-layer readout folds for the Dashboard page (ARCH.md §8.1): the
//! typed observation read behind the summary cards lives here so the render
//! module only paints the folded readout string.

use taskmanager_core::core::metrics::CpuMetrics;

use crate::gpui_app::formatting;

/// CPU summary-card readout: `"{:.1}%"` for a current observation, the shared
/// missing-value dash when the provider has none.
pub(super) fn cpu_summary_readout(cpu: &CpuMetrics) -> String {
    cpu.current_global_usage_pct()
        .map_or_else(formatting::missing_value, |value| format!("{value:.1}%"))
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_dashboard_readouts_tests.rs"]
mod tests;
