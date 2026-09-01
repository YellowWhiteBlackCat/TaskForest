//! Process-table numeric and local start-time formatting.

use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_shell::presentation::cpu_time_compact;

/// Disk-throughput cell on the Drive ladder (a `/s` rate).
pub(super) fn format_bytes_rate(units: UnitPreferences, bytes: u64) -> String {
    units.format_quantity(bytes, QuantityFamily::Drive, true)
}

/// Memory/swap cell on the Memory ladder.
pub(super) fn format_memory(units: UnitPreferences, bytes: u64) -> String {
    units.format_quantity(bytes, QuantityFamily::Memory, false)
}

pub(super) fn format_cpu_percent(value: f32) -> String {
    format!("{value:.1}%")
}

/// `None` (typed unavailable/unknown) renders as "—" instead of a believable
/// zero; `Some` delegates to the caller's formatter.
pub(super) fn optional_u64_dash(value: Option<u64>, format: fn(u64) -> String) -> String {
    value.map_or_else(crate::gpui_app::formatting::missing_value, format)
}

pub(super) fn optional_u32_dash(value: Option<u32>) -> String {
    value.map_or_else(crate::gpui_app::formatting::missing_value, |value| {
        value.to_string()
    })
}

pub(super) fn optional_i32_dash(value: Option<i32>, format: fn(i32) -> String) -> String {
    value.map_or_else(crate::gpui_app::formatting::missing_value, format)
}

pub(super) fn optional_f32_dash(value: Option<f32>, format: fn(f32) -> String) -> String {
    value.map_or_else(crate::gpui_app::formatting::missing_value, format)
}

/// CpuTime cell: the shell's width-bounded compact ladder (`{d}d {h}h` past a
/// day, `{h}h {m}m`, `{m}m {s}s`, `{s}s`; nothing accumulated renders the
/// shared dash). The ladder lives in one place so the desktop table can never
/// drift from the other frontends.
pub(super) fn format_cpu_time(seconds: u64) -> String {
    cpu_time_compact(seconds)
}

pub(super) fn format_nice(nice: i32) -> String {
    if nice > 0 {
        format!("+{nice}")
    } else {
        nice.to_string()
    }
}

pub(super) fn format_start_time(
    seconds: Option<u64>,
    rules: &taskmanager_core::core::time::LocalTimeRulesObservation,
) -> String {
    taskmanager_shell::presentation::start_clock_local(seconds, rules)
}
