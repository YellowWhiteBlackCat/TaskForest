//! Central byte-count → display formatting for the whole UI.
//!
//! Every `u64` byte count that reaches a string (or a proportion derived from
//! byte counts) is converted to `f64` inside this module — no caller does its
//! own `bytes as f64`.
//!
//! # Neutral single source
//!
//! Preference-aware quantity formatting lives in `taskmanager-core`
//! (`core::units`, ADR-020 single-source family): `DisplayUnits` is a
//! compatibility shell over [`UnitPreferences`]. Drive and Network families
//! delegate to the core ladder byte-for-byte; the Memory family stays on the
//! legacy Mission Center ladder until its call sites (outside this file) are
//! replaced in the follow-up wave.
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

/// 1024³ — bytes in one gibibyte.
pub const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
/// 1024² — bytes in one mebibyte.
pub const MIB: f64 = 1024.0 * 1024.0;
/// 1024 — bytes in one kibibyte.
pub const KIB: f64 = 1024.0;

const GIB_BYTES: u64 = 1024 * 1024 * 1024;
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

/// The three Mission Center performance-page quantity families. The provider
/// always publishes byte counts; this enum selects only the presentation unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UnitKind {
    Memory,
    Drive,
    Network,
}

/// Per-window Performance display preferences. These are deliberately owned by
/// the GPUI presentation layer: Config carries the six serializable booleans,
/// while providers and application ports never learn about a user's unit
/// choice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DisplayUnits {
    pub(crate) memory_use_bytes: bool,
    pub(crate) memory_use_base2: bool,
    pub(crate) drive_use_bytes: bool,
    pub(crate) drive_use_base2: bool,
    pub(crate) network_use_bytes: bool,
    pub(crate) network_use_base2: bool,
}

impl Default for DisplayUnits {
    fn default() -> Self {
        Self {
            memory_use_bytes: true,
            memory_use_base2: true,
            drive_use_bytes: true,
            drive_use_base2: true,
            network_use_bytes: false,
            network_use_base2: false,
        }
    }
}

/// One immutable render-entry snapshot for all Performance presentation
/// preferences. Numeric units and graph behavior stay together at the UI
/// boundary while the underlying telemetry remains provider-owned and raw.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub(crate) struct PerformanceSettings {
    pub(crate) units: DisplayUnits,
    pub(crate) graph: GraphSettings,
}

impl DisplayUnits {
    fn settings(self, kind: UnitKind) -> (bool, bool) {
        match kind {
            UnitKind::Memory => (self.memory_use_bytes, self.memory_use_base2),
            UnitKind::Drive => (self.drive_use_bytes, self.drive_use_base2),
            UnitKind::Network => (self.network_use_bytes, self.network_use_base2),
        }
    }

    /// The neutral `taskmanager-core` preference view of this shell.
    fn preferences(self) -> UnitPreferences {
        UnitPreferences {
            memory_use_bytes: self.memory_use_bytes,
            memory_use_base2: self.memory_use_base2,
            drive_use_bytes: self.drive_use_bytes,
            drive_use_base2: self.drive_use_base2,
            network_use_bytes: self.network_use_bytes,
            network_use_base2: self.network_use_base2,
        }
    }

    fn family(kind: UnitKind) -> QuantityFamily {
        match kind {
            UnitKind::Memory => QuantityFamily::Memory,
            UnitKind::Drive => QuantityFamily::Drive,
            UnitKind::Network => QuantityFamily::Network,
        }
    }

    /// Format a provider byte count on the neutral core ladder. Drive and
    /// Network delegate to the core single source (byte-identical with the
    /// TUI/Iced frontends for the same preferences). Memory stays on the
    /// Mission Center ladder for this wave: the memory-readout call sites and
    /// their pinned tests live outside this file (`perf_views.rs`,
    /// `perf_views/memory_details.rs`) and are replaced in the follow-up
    /// call-site wave together with those tests.
    pub(crate) fn format(self, value_bytes: u64, kind: UnitKind, per_second: bool) -> String {
        match kind {
            UnitKind::Memory => self.format_f64(value_bytes as f64, kind, per_second),
            UnitKind::Drive | UnitKind::Network => units::format_quantity(
                value_bytes,
                Self::family(kind),
                per_second,
                &self.preferences(),
            ),
        }
    }

    /// Used/total pair on the same family ladder as [`Self::format`].
    pub(crate) fn format_pair(
        self,
        used_bytes: u64,
        total_bytes: u64,
        kind: UnitKind,
        per_second: bool,
    ) -> String {
        match kind {
            UnitKind::Memory => format!(
                "{} / {}",
                self.format(used_bytes, kind, per_second),
                self.format(total_bytes, kind, per_second)
            ),
            UnitKind::Drive | UnitKind::Network => units::format_quantity_pair(
                used_bytes,
                total_bytes,
                Self::family(kind),
                per_second,
                &self.preferences(),
            ),
        }
    }

    /// A megabyte-valued network graph sample on the neutral Network ladder.
    pub(crate) fn format_network_graph_megabytes(self, value_megabytes: f32) -> String {
        units::format_quantity_f64(
            f64::from(value_megabytes) * 1_000_000.0,
            QuantityFamily::Network,
            true,
            &self.preferences(),
        )
    }

    /// A megabyte-valued drive graph sample on the neutral Drive ladder —
    /// the disk page's read/write throughput series share the decimal-MB/s
    /// coordinate space the network graphs established, so the split
    /// directions' hover/badge/summary text projects through the same
    /// preference-aware ladder.
    pub(crate) fn format_drive_graph_megabytes(self, value_megabytes: f32) -> String {
        units::format_quantity_f64(
            f64::from(value_megabytes) * 1_000_000.0,
            QuantityFamily::Drive,
            true,
            &self.preferences(),
        )
    }

    /// Signed memory rate (MiB/s graph sample) — Memory family, still on the
    /// Mission Center ladder this wave (see [`Self::format`]).
    pub(crate) fn format_signed_memory_rate_mib(self, rate_mib_per_second: f32) -> String {
        let sign = if rate_mib_per_second < 0.0 {
            "−"
        } else {
            "+"
        };
        let magnitude = self.format_f64(
            f64::from(rate_mib_per_second.abs()) * MIB_BYTES as f64,
            UnitKind::Memory,
            true,
        );
        format!("{sign}{magnitude}")
    }

    fn format_f64(self, value_bytes: f64, kind: UnitKind, per_second: bool) -> String {
        if !value_bytes.is_finite() {
            return missing_value();
        }
        let (use_bytes, use_binary) = self.settings(kind);
        format_advanced(value_bytes, use_bytes, use_binary, per_second)
    }
}

/// Graph families used by the shared Performance graph layout. Network and
/// drive-rate samples stay in their historical decimal-MB coordinate space;
/// the formatter projects hover/badge/summary text into the selected unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GraphUnit {
    Percent,
    NetworkRate(DisplayUnits),
    /// Split-direction disk throughput (read/write, decimal MB/s samples) on
    /// the Drive ladder.
    DriveRate(DisplayUnits),
    Rpm,
    Watts,
    Temperature,
    /// Clock magnitude (GPU core MHz) — the GPU headline chart's frequency
    /// family (ADR-034 stage 2).
    Megahertz,
}

fn format_advanced(
    value_bytes: f64,
    use_bytes: bool,
    use_binary: bool,
    per_second: bool,
) -> String {
    const UNITS: [&str; 9] = ["", "K", "M", "G", "T", "P", "E", "Z", "Y"];
    let divisor = if use_binary { 1024.0 } else { 1000.0 };
    let (mut value, label) = if use_bytes {
        (value_bytes, if per_second { "/s" } else { "" })
    } else {
        (value_bytes * 8.0, if per_second { "ps" } else { "" })
    };
    let mut exponent = 0_usize;

    // Mission Center deliberately starts at kilo-units so raw bytes/bits do
    // not dominate compact readouts.
    while exponent < 1 {
        value /= divisor;
        exponent += 1;
    }
    while value >= divisor && exponent < UNITS.len() - 1 {
        value /= divisor;
        exponent += 1;
    }
    let decimals = if exponent > 1 {
        if value.abs() < 10.0 {
            2
        } else if value.abs() < 100.0 {
            1
        } else {
            0
        }
    } else {
        0
    };
    let unit = if use_bytes { "B" } else { "b" };
    let binary = if use_binary { "i" } else { "" };
    format!(
        "{value:.decimals$} {}{binary}{unit}{label}",
        UNITS[exponent],
    )
}

/// `value / unit` in float space with the integer part computed in integer
/// space first, so the whole units stay exact beyond 2^53 (the remainder is
/// `< unit < 2^53` by construction and converts exactly).
#[inline]
fn split_units(value: u64, unit: u64) -> f64 {
    (value / unit) as f64 + (value % unit) as f64 / unit as f64
}

/// Bytes → gibibytes (binary, 1024³).
#[inline]
pub fn bytes_to_gib(bytes: u64) -> f64 {
    split_units(bytes, GIB_BYTES)
}

/// Bytes → mebibytes (binary, 1024²).
#[inline]
pub fn bytes_to_mib(bytes: u64) -> f64 {
    split_units(bytes, MIB_BYTES)
}

/// Adaptive human string: `"{:.1} GiB"` at ≥ 1 GiB, `"{:.1} MiB"` at ≥ 1 MiB,
/// `"{:.1} KiB"` at ≥ 1 KiB, else `"{bytes} B"`. Small counts keep their own
/// unit instead of collapsing to `"0.0 MiB"`.
///
/// The SINGLE implementation lives in `taskmanager-shell` (ADR-020): this is
/// a compatibility forwarder so the GPUI frontend renders byte counts
/// identically to the TUI and iced frontends.
pub fn bytes_to_human(bytes: u64) -> String {
    taskmanager_shell::presentation::bytes(bytes)
}

/// Two-tier health/capacity string: `"{:.1} GiB"` at ≥ 1 GiB, else
/// `"{:.1} MiB"` — the legacy filesystem "free space" readout, where sub-MiB
/// counts legitimately read as `0.0 MiB`.
pub fn format_gib_mib(bytes: u64) -> String {
    if bytes_to_gib(bytes) >= 1.0 {
        format!("{:.1} GiB", bytes_to_gib(bytes))
    } else {
        format!("{:.1} MiB", bytes_to_mib(bytes))
    }
}

/// `"{:.1} GiB"` for a single byte count.
pub fn format_gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes_to_gib(bytes))
}

/// `"{:.0} GiB"` for a single byte count (whole-gibibyte readouts).
pub fn format_gib_whole(bytes: u64) -> String {
    format!("{:.0} GiB", bytes_to_gib(bytes))
}

/// `"{:.0} MiB"` for a single byte count (whole-mebibyte readouts).
pub fn format_mib_whole(bytes: u64) -> String {
    format!("{:.0} MiB", bytes_to_mib(bytes))
}

/// `"{:.2} MiB"` for a single byte count (fractional mebibyte readouts).
pub fn format_mib_2(bytes: u64) -> String {
    format!("{:.2} MiB", bytes_to_mib(bytes))
}

/// Decimal megabytes, `"{:.1} MB"` — the memory-column convention shared by the
/// process table and the properties dialog.
pub fn format_mb_decimal(bytes: u64) -> String {
    format!("{:.1} MB", split_units(bytes, 1_000_000))
}

/// Decimal memory readout: `"{:.1} GB"` at ≥ 1 GB, else `"{:.0} MB"` — the
/// process-table memory column convention.
pub fn format_decimal_memory(bytes: u64) -> String {
    let megabytes = split_units(bytes, 1_000_000);
    if megabytes >= 1024.0 {
        format!("{:.1} GB", split_units(bytes, 1_000_000_000))
    } else {
        format!("{megabytes:.0} MB")
    }
}

/// `"{:.1} / {:.1} GiB"` for a used/total pair.
pub fn format_gib_pair(used: u64, total: u64) -> String {
    format!("{:.1} / {:.1} GiB", bytes_to_gib(used), bytes_to_gib(total))
}

/// Percentage share of `used` within `total` (`0..=100`). `total` must be > 0;
/// callers guard for zero totals before calling.
pub fn bytes_percent(used: u64, total: u64) -> f64 {
    bytes_to_gib(used) / bytes_to_gib(total) * 100.0
}

/// Decimal throughput: `"{:.1} MB/s"` at ≥ 1 MB/s, else `"{:.0} KB/s"`.
pub fn format_bytes_rate(bytes_per_sec: u64) -> String {
    let megabytes = split_units(bytes_per_sec, 1_000_000);
    if megabytes >= 1.0 {
        format!("{megabytes:.1} MB/s")
    } else {
        format!("{:.0} KB/s", split_units(bytes_per_sec, 1_000))
    }
}

/// Decimal link rate from bytes-per-second: `"{:.1} Mbps"` / `"{:.0} Kbps"` /
/// `"{bits} bps"` (bits = bytes × 8).
pub fn format_bit_rate(bytes_per_sec: u64) -> String {
    let bits = bytes_per_sec.saturating_mul(8);
    let megabits = split_units(bits, 1_000_000);
    if megabits >= 1.0 {
        format!("{megabits:.1} Mbps")
    } else if split_units(bits, 1_000) >= 1.0 {
        format!("{:.0} Kbps", split_units(bits, 1_000))
    } else {
        format!("{bits} bps")
    }
}

/// Decimal disk throughput: `"{:.1} GB/s"` (sidebars and aggregate readouts).
pub fn format_gigabytes_per_sec(bytes_per_sec: u64) -> String {
    format!("{:.1} GB/s", split_units(bytes_per_sec, 1_000_000_000))
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_formatting_tests.rs"]
mod tests;
