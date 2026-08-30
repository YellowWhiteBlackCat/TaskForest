//! Central byte-count → display formatting for the whole UI.
//!
//! Every `u64` byte count that reaches a string (or a proportion derived from
//! byte counts) is converted to `f64` inside this module — no caller does its
//! own `bytes as f64`.
//!
//! # Neutral single source
//!
//! Preference-aware quantity formatting lives in `taskmanager-core`
//! (`core::units`, ADR-020 single-source family). GPUI stores and passes the
//! core-owned [`UnitPreferences`] directly; this module only owns graph-sample
//! conversion and unrelated GPUI readout helpers.
//!
//! # Precision contract
//!
//! A bare `u64 as f64` cast is exact only up to 2^53 bytes (~8 PiB); beyond
//! that the count rounds to the nearest representable float and trailing bytes
//! are silently dropped (2^53 + 1 and 2^53 map to the same float). The
//! converters here split each count into an integer whole unit plus a remainder
//! that is smaller than the unit (< 2^30 for the binary units), so the integer
//! part stays exact far beyond 2^53 and the remainder converts exactly. The
//! behaviour below 2^53 is identical to the old `as f64 / unit` expressions.

use crate::gpui_app::graph::GraphSettings;
use taskmanager_core::core::metrics::GpuMetrics;
use taskmanager_core::core::units::{self, QuantityFamily, UnitPreferences};

const MIB_BYTES: u64 = 1024 * 1024;

/// The single missing-value placeholder for GPUI readouts (an em dash),
/// forwarded from the ADR-020 single-source home in `taskmanager-shell`.
/// Views that prefer omitting a row entirely just do not push it.
#[must_use]
pub fn missing_value() -> String {
    taskmanager_shell::presentation::missing_value()
}

/// `{:.2} GHz` from a megahertz count — the single CPU-frequency readout
/// spelling shared by the CPU page, per-core cells, system page, sidebar
/// captions, and properties panels.
#[must_use]
pub fn format_ghz(mhz: u64) -> String {
    format!("{:.2} GHz", mhz as f64 / 1000.0)
}

/// `{:.2} GHz` from a megahertz-valued graph sample (the frequency history
/// series stores MHz as `f32`).
#[must_use]
pub fn format_ghz_sample(megahertz: f32) -> String {
    format!("{:.2} GHz", f64::from(megahertz) / 1000.0)
}

/// Optional megahertz → `"{:.2} GHz"`, with the shared dash for `None` —
/// the single optional frequency readout for spec rows and per-core cells
/// (an uncollected clock never renders as a fabricated `0.00 GHz`).
#[must_use]
pub fn optional_ghz(mhz: Option<u64>) -> String {
    mhz.map_or_else(missing_value, format_ghz)
}

/// GPUI text projection of the shared product-first GPU identity. The
/// positional fallback is frontend-local because it passes through this
/// renderer's locale catalog; the product/brand precedence itself is owned by
/// `taskmanager-shell`.
#[must_use]
pub(crate) fn gpu_identity_text(gpu: &GpuMetrics, index: usize) -> (String, String) {
    let identity = taskmanager_shell::presentation::gpu_display_identity(gpu);
    let title = identity.headline.map_or_else(
        || format!("{} {index}", taskmanager_application::i18n::t("common.gpu")),
        str::to_owned,
    );
    let subtitle = identity.qualifier.unwrap_or_default().to_owned();
    (title, subtitle)
}

/// One immutable render-entry snapshot for all Performance presentation
/// preferences. Numeric units and graph behavior stay together at the UI
/// boundary while the underlying telemetry remains provider-owned and raw.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub(crate) struct PerformanceSettings {
    pub(crate) units: UnitPreferences,
    pub(crate) graph: GraphSettings,
}

/// Project a megabyte-valued network graph sample through the core ladder.
#[must_use]
pub(crate) fn format_network_graph_megabytes(
    preferences: UnitPreferences,
    value_megabytes: f32,
) -> String {
    units::format_quantity_f64(
        f64::from(value_megabytes) * 1_000_000.0,
        QuantityFamily::Network,
        true,
        &preferences,
    )
}

/// Project a megabyte-valued drive graph sample through the core ladder.
#[must_use]
pub(crate) fn format_drive_graph_megabytes(
    preferences: UnitPreferences,
    value_megabytes: f32,
) -> String {
    units::format_quantity_f64(
        f64::from(value_megabytes) * 1_000_000.0,
        QuantityFamily::Drive,
        true,
        &preferences,
    )
}

/// Project a signed MiB/s memory graph sample through the core ladder.
#[must_use]
pub(crate) fn format_signed_memory_rate_mib(
    preferences: UnitPreferences,
    rate_mib_per_second: f32,
) -> String {
    let sign = if rate_mib_per_second < 0.0 {
        "−"
    } else {
        "+"
    };
    let magnitude = units::format_quantity_f64(
        f64::from(rate_mib_per_second.abs()) * MIB_BYTES as f64,
        QuantityFamily::Memory,
        true,
        &preferences,
    );
    format!("{sign}{magnitude}")
}

/// Graph families used by the shared Performance graph layout. Network and
/// drive-rate samples stay in their historical decimal-MB coordinate space;
/// the formatter projects hover/badge/summary text into the selected unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GraphUnit {
    Percent,
    NetworkRate(UnitPreferences),
    /// Split-direction disk throughput (read/write, decimal MB/s samples) on
    /// the Drive ladder.
    DriveRate(UnitPreferences),
    Rpm,
    Watts,
    Temperature,
    /// Clock magnitude (GPU core MHz) — the GPU headline chart's frequency
    /// family (ADR-034 stage 2).
    Megahertz,
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_formatting_tests.rs"]
mod tests;
