//! Unit-matrix formatters for the Performance device views.
//!
//! The persisted Settings Units matrix (bytes/bits × base-2/base-10 per
//! family) resolves into these pure text formatters; the device renderers
//! (`perf_memory`, `perf_disks`, `perf_networks`) read `app.prefs.units` at
//! render entry and pass the pair down. The formatting itself is the
//! `taskmanager-core` single source (`core::units`, reached through the
//! `taskmanager-application` re-export because the dependency firewall
//! forbids a direct core edge in this crate): every frontend renders the
//! same string for the same value + preference pair. These wrappers keep the
//! historical `(value, use_bytes, use_base2)` call shape.

use taskmanager_application::units::format_quantity_with;
use taskmanager_application::{CpuTemperatureSource, i18n::t};
use taskmanager_shell::presentation::missing_value;

/// Format a memory quantity honoring the persisted unit preferences: bytes
/// (`12.0 GiB`) or bits (`98.3 Gib`), each on the base-2 ladder or the
/// base-10 ladder (`KB`/`Kb`). Delegates to the neutral core single source.
#[must_use]
pub(super) fn memory_text_pref(value: u64, use_bytes: bool, use_base2: bool) -> String {
    format_quantity_with(value, use_bytes, use_base2, false)
}

/// Format a drive or network quantity (bytes or the 8× bits equivalent) on
/// the base-2 or base-10 ladder — the same preference pair as memory, shared
/// by the disk and network device views. Delegates to the neutral core
/// single source.
#[must_use]
pub(super) fn quantity_text_pref(value: u64, use_bytes: bool, use_base2: bool) -> String {
    format_quantity_with(value, use_bytes, use_base2, false)
}

/// The optional variant: an unavailable quantity renders an honest dash,
/// never a fabricated zero.
#[must_use]
pub(super) fn quantity_text_optional(
    value: Option<u64>,
    use_bytes: bool,
    use_base2: bool,
) -> String {
    value.map_or_else(missing_value, |value| {
        quantity_text_pref(value, use_bytes, use_base2)
    })
}

pub(super) fn observed_frequency(frequency_mhz: Option<u64>) -> String {
    frequency_mhz.map_or_else(missing_value, |mhz| format!("{mhz} MHz"))
}

pub(super) fn observed_temperature(temperature_c: Option<f32>) -> String {
    temperature_c.map_or_else(missing_value, |value| format!("{value:.0}°C"))
}

/// One CPU temperature readout qualified by its typed source: a
/// labeled-fallback tier (a CPU-package-labeled channel on another hwmon
/// chip, or an ACPI thermal zone) appends the source note so the reading
/// never masquerades as a dedicated CPU sensor chip; native chips and
/// missing values keep the plain readout. Mirrors the iced/gpui folds.
pub(super) fn observed_temperature_for_source(
    temperature_c: Option<f32>,
    source: CpuTemperatureSource,
) -> String {
    let readout = observed_temperature(temperature_c);
    match (temperature_source_note_key(source), temperature_c) {
        (Some(key), Some(_)) => format!("{readout} · {}", t(key)),
        _ => readout,
    }
}

/// The i18n qualifier for a fallback temperature source, if it needs one.
fn temperature_source_note_key(source: CpuTemperatureSource) -> Option<&'static str> {
    match source {
        CpuTemperatureSource::PackageHwmon => Some("cpu.temperature_source.package_hwmon"),
        CpuTemperatureSource::ThermalZone => Some("cpu.temperature_source.thermal_zone"),
        _ => None,
    }
}

pub(super) fn observed_percentage(percentage: Option<f32>) -> String {
    percentage.map_or_else(missing_value, |value| format!("{value:.1}%"))
}

#[cfg(test)]
#[path = "../../tests/gui/ui/units_tests.rs"]
mod tests;
