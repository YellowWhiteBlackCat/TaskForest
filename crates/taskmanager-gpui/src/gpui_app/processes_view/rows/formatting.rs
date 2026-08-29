//! Process-table numeric and local start-time formatting.

pub(super) fn format_bytes_rate(bytes: u64) -> String {
    crate::gpui_app::formatting::format_bytes_rate(bytes)
}

pub(super) fn format_memory(bytes: u64) -> String {
    crate::gpui_app::formatting::format_decimal_memory(bytes)
}

pub(super) fn format_cpu_percent(value: f32) -> String {
    format!("{value:.1}%")
}

/// Format an aggregate CPU value from the same rounded values shown by its
/// member rows. Summing the raw samples and rounding only once makes a group
/// such as `9.1 + 3.3` appear as `12.5` even though the visible members add to
/// `12.4`; the aggregate row must be checkable from what is on screen.
pub(super) fn format_additive_cpu(values: impl IntoIterator<Item = Option<f32>>) -> String {
    let mut total = 0.0_f64;
    let mut available = false;
    for value in values
        .into_iter()
        .flatten()
        .filter(|value| value.is_finite())
    {
        total += (f64::from(value) * 10.0).round() / 10.0;
        available = true;
    }
    if available {
        format_cpu_percent(total as f32)
    } else {
        crate::gpui_app::formatting::missing_value()
    }
}

/// Format an aggregate memory value by adding the already-rounded member
/// readouts. Process rows use whole MB below 1 GB and one decimal GB above it;
/// retaining the member unit here prevents the root from differing by one MB
/// solely because every child was rounded independently.
pub(super) fn format_additive_memory(values: impl IntoIterator<Item = Option<u64>>) -> String {
    let mut total = 0.0_f64;
    let mut available = false;
    let mut has_gb_member = false;
    for bytes in values.into_iter().flatten() {
        let megabytes = bytes as f64 / 1_000_000.0;
        if megabytes >= 1024.0 {
            total += (megabytes / 1000.0 * 10.0).round() / 10.0 * 1000.0;
            has_gb_member = true;
        } else {
            total += megabytes.round();
        }
        available = true;
    }
    if !available {
        return crate::gpui_app::formatting::missing_value();
    }
    if has_gb_member {
        format!("{:.1} GB", total / 1000.0)
    } else {
        format!("{total:.0} MB")
    }
}

/// Format an aggregate byte-rate value by adding the rounded unit displayed
/// for each member (`KB/s` or `MB/s`). This keeps disk columns additive for the
/// same reason as [`format_additive_memory`].
pub(super) fn format_additive_rate(values: impl IntoIterator<Item = Option<u64>>) -> String {
    let mut total = 0.0_f64;
    let mut available = false;
    let mut has_mb_member = false;
    for bytes in values.into_iter().flatten() {
        let megabytes = bytes as f64 / 1_000_000.0;
        if megabytes >= 1.0 {
            total += (megabytes * 10.0).round() / 10.0;
            has_mb_member = true;
        } else {
            total += (bytes as f64 / 1_000.0).round() / 1000.0;
        }
        available = true;
    }
    if !available {
        return crate::gpui_app::formatting::missing_value();
    }
    if has_mb_member {
        format!("{total:.1} MB/s")
    } else {
        format!("{:.0} KB/s", total * 1000.0)
    }
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

pub(super) fn format_cpu_time(seconds: u64) -> String {
    if seconds == 0 {
        return crate::gpui_app::formatting::missing_value();
    }
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    if days > 0 {
        // Drop the minutes segment once a process is at least a day old: the
        // CpuTime column is width-bounded, and "{days}d {hours}h" stays well
        // inside it even for multi-year uptimes (where "{minutes}m" would push
        // the right-aligned mono digits leftward into DiskWrite).
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
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
