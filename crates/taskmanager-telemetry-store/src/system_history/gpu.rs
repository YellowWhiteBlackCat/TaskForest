//! Typed GPU scalar points retained by the correlated history.

use taskmanager_core::GpuMetrics;

/// One timestamped GPU scalar point.
///
/// Fields intentionally keep their source units. The GPUI projection may
/// derive percentages for memory graphs, but the history never loses the byte
/// or MHz values that make that projection auditable.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GpuMetricPoint {
    pub utilization_pct: Option<f32>,
    pub temperature_c: Option<f32>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub dedicated_memory_used_bytes: Option<u64>,
    pub dedicated_memory_total_bytes: Option<u64>,
    pub shared_memory_used_bytes: Option<u64>,
    pub shared_memory_total_bytes: Option<u64>,
    pub power_w: Option<f32>,
    pub frequency_mhz: Option<u64>,
    pub idle_residency_pct: Option<f32>,
}

impl GpuMetricPoint {
    /// Project one current typed GPU row into history without turning a
    /// missing or non-finite scalar into a measured zero.
    #[must_use]
    pub fn from_metrics(metrics: &GpuMetrics) -> Self {
        Self {
            utilization_pct: finite(metrics.current_utilization_pct()),
            temperature_c: finite(metrics.current_temperature_c()),
            memory_used_bytes: metrics.current_memory_used_bytes(),
            memory_total_bytes: metrics.current_memory_total_bytes(),
            dedicated_memory_used_bytes: metrics.current_dedicated_vram_used_bytes(),
            dedicated_memory_total_bytes: metrics.current_dedicated_vram_total_bytes(),
            shared_memory_used_bytes: metrics.current_shared_vram_used_bytes(),
            shared_memory_total_bytes: metrics.current_shared_vram_total_bytes(),
            power_w: finite(metrics.current_power_w()),
            frequency_mhz: metrics.current_frequency_mhz(),
            idle_residency_pct: finite(metrics.current_idle_residency_pct()),
        }
    }
}

fn finite(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite())
}
