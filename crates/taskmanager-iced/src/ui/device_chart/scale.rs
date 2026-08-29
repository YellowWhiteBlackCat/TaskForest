//! Typed unit/scaling projection for Iced per-device graphs and hover readouts.
//! The parent canvas module owns rendering; this file keeps the unit family and
//! finite-peak policy in one testable seam.

use super::*;
use taskmanager_shell::presentation::missing_value;

/// The Y-axis scaling rule AND unit family for a per-device mini-graph,
/// decoupled from the system-wide [`MetricSeries`] identity because the
/// per-device windows (per-disk/net throughput, per-GPU utilization, per-fan
/// RPM, per-battery charge %) are read through their own `*_for` accessors
/// rather than through `MetricSeries` — battery charge % and fan RPM have no
/// `MetricSeries` variant by design (they bypass the system-wide series enum).
/// [`Self::Percent`] pins the frame ceiling at 100 (CPU/GPU/memory utilization,
/// battery charge %); every other variant tracks the finite peak across the
/// window so magnitude traces rise with the value instead of clamping flat.
/// Besides the ceiling, the scale also picks the summary/hover-readout unit
/// ([`scale_unit_suffix`]) — the unit-carrying magnitude variants
/// ([`Self::Watts`] / [`Self::Celsius`] / [`Self::Megahertz`]) exist so a
/// power/temperature/clock graph never prints a bare number (GPUI's
/// `GraphUnit` parity); [`Self::AutoPeak`] is the unitless fallback for
/// call sites that have not migrated to a unit-carrying variant yet. The
/// [`From<MetricSeries>`] keeps the fixed system-wide metric histories on the
/// same rule as the per-device graphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeviceMetricScale {
    /// Fixed 0..100 ceiling (utilization %, battery charge %).
    Percent,
    /// Ceiling tracks the finite peak across the window (bytes/sec, RPM, °C, MHz).
    AutoPeak,
    /// Bytes-per-second magnitude whose summary/hover readout formats through
    /// the resolved Drive or Network unit pair (bytes-vs-bits × base-2-vs-
    /// base-10) the call site owns — a disk graph passes the drive pair, a NIC
    /// graph the network pair (GPUI's `GraphUnit::NetworkRate(DisplayUnits)`
    /// parity) — so the graph readouts follow the same persisted preference
    /// as the scalar rows instead of hardcoding binary bytes.
    BytesPerSecond { use_bytes: bool, use_base2: bool },
    /// Fan-speed magnitude with an RPM suffix.
    Rpm,
    /// Power magnitude (GPU/CPU/battery watts) with a ` W` suffix.
    Watts,
    /// Temperature magnitude (GPU/CPU/fan °C) with a ` °C` suffix.
    Celsius,
    /// Clock magnitude (GPU/CPU MHz) with a ` MHz` suffix.
    Megahertz,
}

impl From<TrendSeries> for DeviceMetricScale {
    fn from(series: TrendSeries) -> Self {
        match series {
            TrendSeries::CpuUsagePercent
            | TrendSeries::MemoryUsagePercent
            | TrendSeries::GpuUsagePercent
            | TrendSeries::DiskActiveTimePct => Self::Percent,
            TrendSeries::DiskBytesPerSec | TrendSeries::NetworkBytesPerSec => {
                Self::BytesPerSecond {
                    use_bytes: true,
                    use_base2: true,
                }
            }
            TrendSeries::CpuTemperatureC => Self::Celsius,
            TrendSeries::CpuFrequencyMhz => Self::Megahertz,
            TrendSeries::CpuPowerW => Self::Watts,
        }
    }
}

/// The value that maps to the TOP of the frame: `100.0` for a percentage-typed
/// series, and the finite positive peak for a magnitude series. `0.0` when such
/// a window is empty/all-zero (idle). Accepts either a [`DeviceMetricScale`]
/// directly or any [`MetricSeries`] via the bridge.
#[must_use]
pub(crate) fn series_max(scale: impl Into<DeviceMetricScale>, samples: &[f32]) -> f32 {
    match scale.into() {
        DeviceMetricScale::Percent => PERCENT_MAX,
        DeviceMetricScale::AutoPeak
        | DeviceMetricScale::BytesPerSecond { .. }
        | DeviceMetricScale::Rpm
        | DeviceMetricScale::Watts
        | DeviceMetricScale::Celsius
        | DeviceMetricScale::Megahertz => finite_peak(samples),
    }
}

#[must_use]
fn scale_unit_suffix(scale: DeviceMetricScale) -> &'static str {
    match scale {
        DeviceMetricScale::Percent => "%",
        DeviceMetricScale::AutoPeak | DeviceMetricScale::BytesPerSecond { .. } => "",
        DeviceMetricScale::Rpm => " RPM",
        DeviceMetricScale::Watts => " W",
        DeviceMetricScale::Celsius => " \u{b0}C",
        DeviceMetricScale::Megahertz => " MHz",
    }
}

pub(crate) fn summary_value(scale: DeviceMetricScale, value: f32) -> String {
    let unit = scale_unit_suffix(scale);
    match scale {
        DeviceMetricScale::Percent
        | DeviceMetricScale::Rpm
        | DeviceMetricScale::Celsius
        | DeviceMetricScale::Megahertz => format!("{value:.0}{unit}"),
        DeviceMetricScale::Watts | DeviceMetricScale::AutoPeak => format!("{value:.1}{unit}"),
        DeviceMetricScale::BytesPerSecond {
            use_bytes,
            use_base2,
        } => {
            if value.is_finite() && value >= 0.0 {
                format!(
                    "{}/s",
                    quantity_text_pref(value.round() as u64, use_bytes, use_base2)
                )
            } else {
                missing_value()
            }
        }
    }
}

/// The hover pill's text at one sample index, formatted in the graph's unit
/// family. `None` means the index is outside the honest partial-buffer state
/// or is an explicit non-finite history gap.
#[must_use]
pub(crate) fn device_readout_text(
    scale: DeviceMetricScale,
    samples: &[f32],
    index: usize,
) -> Option<String> {
    samples
        .get(index)
        .copied()
        .filter(|value| value.is_finite())
        .map(|value| summary_value(scale, value))
}

pub(crate) fn mini_graph_summary(scale: DeviceMetricScale, samples: &[f32]) -> Option<String> {
    let summary = graph_summary(samples)?;
    Some(format!(
        "{} {} · {} {} · {} {}",
        t("common.latest"),
        summary_value(scale, summary.latest),
        t("common.avg"),
        summary_value(scale, summary.average),
        t("common.peak"),
        summary_value(scale, summary.maximum),
    ))
}
