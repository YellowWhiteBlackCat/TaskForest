//! Renderer-neutral trend vocabulary and reads over the composed-track live
//! history.
//!
//! Behavior seam (ADR-027): the shell track owns the live-graph composition
//! for its frontends, and shell-track renderers (TUI/Iced/Bevy) consume trend
//! data through this vocabulary instead of importing the telemetry-store read
//! model — the storage crate stays outside the renderer dependency set
//! (frontend dependency firewall). The selectors carry renderer vocabulary
//! (`TrendSeries::CpuUsagePercent`); the generation/gap/revision semantics of
//! the underlying windows remain owned by `taskmanager-telemetry-store`. The
//! direct track (GPUI) keeps its sanctioned store-backed composition and is
//! not a consumer of this seam.

use taskmanager_telemetry_store::live_graph::{LiveGraphHistory, MetricSeries};

/// One trend window a renderer may chart, in the canonical order that backs
/// [`TrendSeries::slot`]. A new variant must extend [`TrendSeries::ALL`];
/// the slot round-trip is pinned by test.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum TrendSeries {
    CpuUsagePercent,
    MemoryUsagePercent,
    DiskBytesPerSec,
    NetworkBytesPerSec,
    GpuUsagePercent,
    CpuTemperatureC,
    CpuFrequencyMhz,
    CpuPowerW,
    DiskActiveTimePct,
}

impl TrendSeries {
    /// Every series exactly once, in the canonical slot order.
    pub const ALL: [Self; 9] = [
        Self::CpuUsagePercent,
        Self::MemoryUsagePercent,
        Self::DiskBytesPerSec,
        Self::NetworkBytesPerSec,
        Self::GpuUsagePercent,
        Self::CpuTemperatureC,
        Self::CpuFrequencyMhz,
        Self::CpuPowerW,
        Self::DiskActiveTimePct,
    ];

    /// The storage slot this selector reads.
    #[must_use]
    pub const fn slot(self) -> usize {
        match self {
            Self::CpuUsagePercent => 0,
            Self::MemoryUsagePercent => 1,
            Self::DiskBytesPerSec => 2,
            Self::NetworkBytesPerSec => 3,
            Self::GpuUsagePercent => 4,
            Self::CpuTemperatureC => 5,
            Self::CpuFrequencyMhz => 6,
            Self::CpuPowerW => 7,
            Self::DiskActiveTimePct => 8,
        }
    }

    const fn storage_series(self) -> MetricSeries {
        match self {
            Self::CpuUsagePercent => MetricSeries::CpuUsagePercent,
            Self::MemoryUsagePercent => MetricSeries::MemoryUsagePercent,
            Self::DiskBytesPerSec => MetricSeries::DiskBytesPerSec,
            Self::NetworkBytesPerSec => MetricSeries::NetworkBytesPerSec,
            Self::GpuUsagePercent => MetricSeries::GpuUsagePercent,
            Self::CpuTemperatureC => MetricSeries::CpuTemperatureC,
            Self::CpuFrequencyMhz => MetricSeries::CpuFrequencyMhz,
            Self::CpuPowerW => MetricSeries::CpuPowerW,
            Self::DiskActiveTimePct => MetricSeries::DiskActiveTimePct,
        }
    }
}

/// One system-wide trend window, in percent / bytes-per-second / °C / MHz /
/// W depending on the selector. Absence stays an empty window — the read
/// never fabricates zero-filled samples.
#[must_use]
pub fn window(history: &LiveGraphHistory, series: TrendSeries) -> Vec<f32> {
    history.series(series.storage_series())
}

/// The CPU-utilization trend window, in percent.
#[must_use]
pub fn cpu_usage_percent(history: &LiveGraphHistory) -> Vec<f32> {
    window(history, TrendSeries::CpuUsagePercent)
}

/// The memory-utilization trend window, in percent.
#[must_use]
pub fn memory_usage_percent(history: &LiveGraphHistory) -> Vec<f32> {
    window(history, TrendSeries::MemoryUsagePercent)
}

/// The per-core utilization trend windows, one window per logical core.
#[must_use]
pub fn per_core_usage_percent(history: &LiveGraphHistory) -> Vec<Vec<f32>> {
    history.per_core_usage_series()
}

#[cfg(test)]
#[path = "../../tests/headless/presentation_trend_tests.rs"]
mod tests;
