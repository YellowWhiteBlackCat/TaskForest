//! Neutral unit formatting — the single source for byte/bit × base-2/base-10
//! presentation (the "unit formatting not neutralized" defect).
//!
//! Pure functions over plain data: no I/O, no toolkit types. The output spec
//! is the ladder every frontend already renders on its base-2 bytes path (the
//! historical `presentation::bytes` (taskmanager-shell) single source):
//!
//! * one decimal place above the value tier (`1.5 KiB`, `2.0 GB`),
//! * a plain whole-value tier below the k-unit (`512 B`, `800 b`),
//! * bits are the 8× byte value with a lowercase `b`,
//! * base-2 spells the `i` forms (`KiB`/`MiB`/`GiB`, `Kib`/`Mib`/`Gib`),
//!   base-10 the decimal forms (`KB`/`MB`/`GB`, `Kb`/`Mb`/`Gb`),
//! * a per-second quantity appends `/s` after the unit (`1.5 KiB/s`),
//!   matching the TUI/Iced view-layer suffix convention.
//!
//! The ladder tops out at the g-unit tier (as the shared `bytes` formatter
//! always has); larger magnitudes keep dividing into the g-tier readout.
//!
//! Defaults follow the Mission Center parity contract: memory and drive
//! families render bytes on the base-2 ladder, the network family renders
//! bits on the base-10 ladder.

use super::config::Config;

/// The three Performance quantity families. Providers always publish byte
/// counts; this selects only which preference pair formats a value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuantityFamily {
    Memory,
    Drive,
    Network,
}

/// Pure unit-preference data extracted from the core `Config` unit fields.
/// Carries no toolkit or provider types so every frontend (and the
/// application layer) can share one formatting semantic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitPreferences {
    /// Memory family: `true` renders bytes, `false` renders the 8× bits value.
    pub memory_use_bytes: bool,
    /// Memory family: `true` uses the 1024 ladder, `false` the 1000 ladder.
    pub memory_use_base2: bool,
    /// Drive family: bytes vs bits.
    pub drive_use_bytes: bool,
    /// Drive family: base-2 vs base-10.
    pub drive_use_base2: bool,
    /// Network family: bytes vs bits.
    pub network_use_bytes: bool,
    /// Network family: base-2 vs base-10.
    pub network_use_base2: bool,
}

impl Default for UnitPreferences {
    /// Mission Center parity: memory/drive bytes + base 2, network bits +
    /// base 10. Matches `Config`'s serde defaults and the GPUI
    /// `DisplayUnits::default()` shell.
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

impl From<&Config> for UnitPreferences {
    fn from(config: &Config) -> Self {
        Self {
            memory_use_bytes: config.memory_use_bytes,
            memory_use_base2: config.memory_use_base2,
            drive_use_bytes: config.drive_use_bytes,
            drive_use_base2: config.drive_use_base2,
            network_use_bytes: config.network_use_bytes,
            network_use_base2: config.network_use_base2,
        }
    }
}

impl UnitPreferences {
    /// Resolve a family's `(use_bytes, use_base2)` formatting pair.
    #[must_use]
    pub const fn settings(&self, family: QuantityFamily) -> (bool, bool) {
        match family {
            QuantityFamily::Memory => (self.memory_use_bytes, self.memory_use_base2),
            QuantityFamily::Drive => (self.drive_use_bytes, self.drive_use_base2),
            QuantityFamily::Network => (self.network_use_bytes, self.network_use_base2),
        }
    }

    /// Format one byte-count quantity through the canonical core ladder.
    #[must_use]
    pub fn format_quantity(self, bytes: u64, family: QuantityFamily, per_second: bool) -> String {
        format_quantity(bytes, family, per_second, &self)
    }

    /// Format a used/total pair through the canonical core ladder.
    #[must_use]
    pub fn format_quantity_pair(
        self,
        used: u64,
        total: u64,
        family: QuantityFamily,
        per_second: bool,
    ) -> String {
        format_quantity_pair(used, total, family, per_second, &self)
    }
}

/// Format a memory quantity with the memory preference pair — the canonical
/// memory readout shared by every frontend.
#[must_use]
pub fn format_memory(bytes: u64, prefs: &UnitPreferences) -> String {
    format_quantity(bytes, QuantityFamily::Memory, false, prefs)
}

/// Format a network transfer rate (bytes per second) with the network
/// preference pair — bits/base-10 by default, `/s` appended after the unit.
#[must_use]
pub fn format_byte_rate(bytes_per_sec: u64, prefs: &UnitPreferences) -> String {
    format_quantity(bytes_per_sec, QuantityFamily::Network, true, prefs)
}

/// Format a byte count honoring the family's preference pair, optionally as a
/// per-second rate (`/s` appended after the unit).
#[must_use]
pub fn format_quantity(
    bytes: u64,
    family: QuantityFamily,
    per_second: bool,
    prefs: &UnitPreferences,
) -> String {
    let (use_bytes, use_base2) = prefs.settings(family);
    format_quantity_with(bytes, use_bytes, use_base2, per_second)
}

/// Format a used/total byte-count pair as `"{used} / {total}"`, both sides on
/// the family's preference ladder.
#[must_use]
pub fn format_quantity_pair(
    used: u64,
    total: u64,
    family: QuantityFamily,
    per_second: bool,
    prefs: &UnitPreferences,
) -> String {
    format!(
        "{} / {}",
        format_quantity(used, family, per_second, prefs),
        format_quantity(total, family, per_second, prefs)
    )
}

/// Format a float byte count (graph samples, derived rates) with the family's
/// preference pair. Non-finite input fails closed to the shared missing-value
/// dash rather than rendering a fabricated number.
#[must_use]
pub fn format_quantity_f64(
    value_bytes: f64,
    family: QuantityFamily,
    per_second: bool,
    prefs: &UnitPreferences,
) -> String {
    let (use_bytes, use_base2) = prefs.settings(family);
    if !value_bytes.is_finite() {
        return MISSING_VALUE.to_owned();
    }
    ladder(value_bytes, use_bytes, use_base2, per_second)
}

/// Format a byte count from an already-resolved `(use_bytes, use_base2)`
/// pair — the entry every frontend delegates through,
/// since their preference resolution predates [`UnitPreferences`].
#[must_use]
pub fn format_quantity_with(
    bytes: u64,
    use_bytes: bool,
    use_base2: bool,
    per_second: bool,
) -> String {
    ladder(bytes as f64, use_bytes, use_base2, per_second)
}

/// Convert bytes to binary gibibytes without first rounding the complete byte
/// count to `f64`. The integer quotient remains exact for all `u64` inputs and
/// the remainder is smaller than the binary unit.
#[must_use]
pub fn bytes_to_gib(bytes: u64) -> f64 {
    split_units(bytes, 1024 * 1024 * 1024)
}

/// Convert bytes to binary mebibytes without first rounding the complete byte
/// count to `f64`.
#[must_use]
pub fn bytes_to_mib(bytes: u64) -> f64 {
    split_units(bytes, 1024 * 1024)
}

/// Percentage share of `used` within `total`. A missing/zero denominator is
/// unavailable rather than a numeric zero.
#[must_use]
pub fn bytes_percent(used: u64, total: u64) -> Option<f64> {
    let total = (total > 0).then_some(total)?;
    Some(bytes_to_gib(used) / bytes_to_gib(total) * 100.0)
}

/// The shared missing-value placeholder for non-finite float inputs. Spec pin
/// of [`taskmanager_shell::presentation::MISSING_VALUE`] (core cannot depend
/// on the shell presentation layer).
const MISSING_VALUE: &str = "—";

/// The single magnitude ladder: value in bytes → tiered string. Bits are the
/// 8× value; the k/m/g tiers keep one decimal, the sub-k tier a whole value.
fn ladder(value_bytes: f64, use_bytes: bool, use_base2: bool, per_second: bool) -> String {
    let k = if use_base2 { 1024.0 } else { 1000.0 };
    let m = k * k;
    let g = m * k;
    let value = value_bytes * if use_bytes { 1.0 } else { 8.0 };
    let unit = if use_bytes { "B" } else { "b" };
    let binary = if use_base2 { "i" } else { "" };
    let rate = if per_second { "/s" } else { "" };
    if value >= g {
        format!("{:.1} G{binary}{unit}{rate}", value / g)
    } else if value >= m {
        format!("{:.1} M{binary}{unit}{rate}", value / m)
    } else if value >= k {
        format!("{:.1} K{binary}{unit}{rate}", value / k)
    } else {
        format!("{value:.0} {unit}{rate}")
    }
}

#[inline]
fn split_units(value: u64, unit: u64) -> f64 {
    (value / unit) as f64 + (value % unit) as f64 / unit as f64
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_units_tests.rs"]
mod tests;
