//! Shell-level folds for the shared GPU chart-metric selection
//! (ADR-034 stage 2).
//!
//! `ShellApp` (the Iced/TUI composed track) and `DirectTrackState` (the GPUI
//! direct track) each own one [`GpuChartMetricSelection`] per window. The
//! selection authority, the availability gate, and the generation reset live
//! in the pure presentation contract; this module only binds the composed
//! track's instance to the same viewed-device gate and exposes the read
//! surface renderers consume. Frontends never hold a second selection or
//! re-derive availability (ADR-034 验收约束: “选择态单一权威在 shell”).

use taskmanager_core::core::metrics::GpuMetrics;

use crate::presentation::gpu_chart_metric::{
    GpuChartMetric, GpuChartMetricGate, GpuChartMetricProjection,
};

impl super::ShellApp {
    /// The per-tick fold (ADR-034 stage 2: “每 tick 折叠”): fold the viewed
    /// device's gate — availability from its latest typed point plus its
    /// device generation — into the shared selection. A no-viewed-device
    /// gate leaves the selection untouched. Returns whether the selection
    /// changed.
    pub fn reconcile_gpu_chart_metric(&mut self, gate: &GpuChartMetricGate) -> bool {
        self.gpu_chart_metric.reconcile_gate(gate)
    }

    /// Select one series family through the same gate the projection
    /// renders (the pill/keyboard activation path). Unavailable families
    /// are rejected with no state change. Returns whether the selection
    /// changed.
    pub fn select_gpu_chart_metric(
        &mut self,
        metric: GpuChartMetric,
        gate: &GpuChartMetricGate,
    ) -> bool {
        self.gpu_chart_metric.select_gate(metric, gate)
    }

    /// Advance to the next available family in the fixed vocabulary order
    /// (the TUI `g` cycle). Returns whether the selection changed.
    pub fn cycle_gpu_chart_metric(&mut self, gate: &GpuChartMetricGate) -> bool {
        self.gpu_chart_metric.cycle_gate(gate)
    }

    /// The selector projection every renderer paints for the viewed device.
    /// Unavailable families stay present and explicitly unavailable.
    #[must_use]
    pub fn gpu_chart_metric_projection(
        &self,
        gate: &GpuChartMetricGate,
    ) -> GpuChartMetricProjection {
        self.gpu_chart_metric.projection_gate(gate)
    }

    /// The currently selected family (the chart's series identity).
    #[must_use]
    pub const fn gpu_chart_metric_selected(&self) -> GpuChartMetric {
        self.gpu_chart_metric.selected()
    }
}

/// Derive the viewed-device gate once per tick/render from the GPU row a
/// frontend is viewing (`None` when it is not viewing a GPU). Pure and
/// `Copy` so the read can complete before the fold borrows the shell state.
#[must_use]
pub fn gpu_chart_metric_gate(gpu: Option<&GpuMetrics>) -> GpuChartMetricGate {
    GpuChartMetricGate::for_viewed_gpu(gpu)
}
