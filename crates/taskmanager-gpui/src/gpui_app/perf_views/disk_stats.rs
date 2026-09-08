//! Disk performance-page stat readout construction.
//!
//! Row contract (求同存异): rate-family fields whose absence is a sampling
//! gap keep their row with `None` (the panel renders the shared dash);
//! existence facts (SMART health, removable media) omit their rows when the
//! host does not have them.

use taskmanager_application::i18n;
use taskmanager_core::core::metrics::DiskMetrics;

use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_shell::viewmodel::StatRow;

use super::{
    device_status_i18n_key, drive_rate_str, effective_smart_status, has_smart_fields,
    smart_section_visible,
};

/// The Disk page's undroppable one-line capacity fact: used/total plus the
/// partition census. Lives in the DATA layer (ARCH.md §8.1) because it reads
/// capacity observations; the paint module only formats the resulting line.
/// Honest absence — a disk whose capacity or partition facts are uncollected
/// keeps the dash or omits the segment, never a fabricated zero.
pub(super) fn vital_line(d: &DiskMetrics, units: UnitPreferences) -> String {
    let mut segments: Vec<String> = Vec::new();
    match (d.current_capacity_bytes(), d.current_available_bytes()) {
        (Some(total), Some(free)) if total > 0 => {
            let used = total.saturating_sub(free).min(total);
            segments.push(format!(
                "{} / {}",
                units.format_quantity(used, QuantityFamily::Drive, false),
                units.format_quantity(total, QuantityFamily::Drive, false),
            ));
        }
        _ => segments.push(crate::gpui_app::formatting::missing_value()),
    }
    if !d.partitions.is_empty() {
        segments.push(format!(
            "{} {}",
            d.partitions.len(),
            i18n::t("disk.partitions").to_lowercase(),
        ));
    }
    segments.join(" · ")
}

pub(super) fn disk_stats(
    d: &DiskMetrics,
    units: UnitPreferences,
    temperature_samples: &[f32],
) -> Vec<StatRow> {
    let mut stats = vec![
        StatRow::text(
            i18n::t("device.status"),
            Some(i18n::t(device_status_i18n_key(d.device_state.status)).into()),
        ),
        StatRow::text(
            i18n::t("disk.active_time"),
            d.current_active_time_pct()
                .map(|value| format!("{:.0}%", value.round())),
        ),
        StatRow::text(
            i18n::t("disk.read"),
            d.current_read_bytes_per_sec()
                .map(|value| drive_rate_str(units, value)),
        ),
        StatRow::text(
            i18n::t("disk.write"),
            d.current_write_bytes_per_sec()
                .map(|value| drive_rate_str(units, value)),
        ),
        StatRow::text(
            i18n::t("disk.iops"),
            d.current_iops().map(|value| value.to_string()),
        ),
        StatRow::text(
            i18n::t("disk.response"),
            d.current_response_time_ms()
                .map(|value| format!("{value:.2} ms")),
        ),
        StatRow::text(
            i18n::t("disk.queue_depth"),
            d.current_average_queue_depth()
                .map(|value| format!("{value:.2}")),
        ),
        StatRow::text(
            i18n::t("disk.service_time"),
            d.current_service_time_ms()
                .map(|value| format!("{value:.2} ms")),
        ),
        StatRow::text(
            i18n::t("disk.capacity"),
            d.current_capacity_bytes()
                .map(|value| units.format_quantity(value, QuantityFamily::Drive, false)),
        ),
        StatRow::text(
            i18n::t("disk.free"),
            d.current_available_bytes()
                .map(|value| units.format_quantity(value, QuantityFamily::Drive, false)),
        ),
        StatRow::text(
            i18n::t("common.type"),
            (!d.disk_type.trim().is_empty()).then(|| d.disk_type.trim().to_string()),
        ),
        StatRow::text(
            i18n::t("disk.filesystem"),
            (!d.fs_type.trim().is_empty()).then(|| d.fs_type.trim().to_string()),
        ),
    ];
    if d.current_read_merges_per_sec().is_some() || d.current_write_merges_per_sec().is_some() {
        stats.push(StatRow::text(
            i18n::t("disk.merged_requests"),
            Some(format!(
                "{} / {}",
                d.current_read_merges_per_sec()
                    .map_or_else(crate::gpui_app::formatting::missing_value, |value| {
                        value.to_string()
                    }),
                d.current_write_merges_per_sec()
                    .map_or_else(crate::gpui_app::formatting::missing_value, |value| {
                        value.to_string()
                    })
            )),
        ));
    }
    if let Some(serial) = d.serial.as_deref().filter(|value| !value.is_empty()) {
        stats.push(StatRow::text(
            i18n::t("disk.serial"),
            Some(serial.to_owned()),
        ));
    }
    if let Some(revision) = d.revision.as_deref().filter(|value| !value.is_empty()) {
        stats.push(StatRow::text(
            i18n::t("disk.revision"),
            Some(revision.to_owned()),
        ));
    }
    // ── NVMe / SMART health (only when the kernel exposes a health node) ──
    // The critical-warning prefix surfaces the most actionable SMART bit the
    // hwmon layer carries; otherwise a plain temperature readout.
    if let Some(temp) = d.smart_temperature_c {
        let warn = d.smart_critical_warning == Some(true);
        let label = if warn {
            format!("{} \u{26a0}", i18n::t("common.temperature"))
        } else {
            i18n::t("common.temperature").to_string()
        };
        let val = match d.smart_temp_critical_c {
            Some(crit) if crit > 0.0 => format!("{:.0} / {:.0} \u{b0}C", temp, crit),
            _ => format!("{:.0} \u{b0}C", temp),
        };
        stats.push(StatRow::text(label, Some(val)));
        // SMART temperature trend from this disk identity's generation-scoped
        // telemetry-store history. Only a window with at least one finite
        // sample renders a row; another disk can never influence it.
        if let Some(trend_row) = temperature_trend_stat_row(temperature_samples) {
            stats.push(trend_row);
        }
    }
    for (index, temperature) in d.smart_temperature_sensors_c.iter().enumerate() {
        if temperature.is_finite() {
            stats.push(StatRow::text(
                format!("{} {}", i18n::t("disk.temperature_sensor"), index + 1),
                Some(format!("{temperature:.0} \u{b0}C")),
            ));
        }
    }
    if let Some(pct) = d.smart_percent_used {
        stats.push(StatRow::text(
            i18n::t("disk.endurance_used"),
            Some(format!("{:.0}%", pct)),
        ));
    }
    if let Some(spare) = d.smart_available_spare_pct {
        let warning_threshold = d.smart_available_spare_threshold_pct.unwrap_or(10.0);
        let value = if spare <= warning_threshold {
            format!("{spare:.0}% ⚠ (≤{warning_threshold:.0}%)")
        } else {
            format!("{spare:.0}%")
        };
        stats.push(StatRow::text(i18n::t("disk.available_spare"), Some(value)));
    }
    if let Some(hours) = d.smart_power_on_hours {
        // Power-on hours → years/days for a glanceable wear figure.
        let days = hours / 24;
        stats.push(StatRow::text(
            i18n::t("disk.power_on"),
            Some(
                i18n::t("disk.power_on_format")
                    .replace("{hours}", &hours.to_string())
                    .replace("{days}", &days.to_string()),
            ),
        ));
    }
    if let Some(count) = d.smart_unsafe_shutdowns {
        stats.push(StatRow::text(
            i18n::t("disk.unsafe_shutdowns"),
            Some(count.to_string()),
        ));
    }
    if smart_section_visible(d) && !has_smart_fields(d) {
        stats.push(StatRow::text(
            i18n::t("disk.smart_status"),
            Some(i18n::t(device_status_i18n_key(effective_smart_status(d))).into()),
        ));
    }
    if d.media_removable() == Some(true) {
        stats.push(StatRow::text(
            i18n::t("disk.removable"),
            Some(i18n::t("common.yes").into()),
        ));
    }
    stats
}

/// Latest/average/peak summary of one disk's SMART temperature window (°C),
/// mirroring the TUI's `device_summary_line` semantics. `None` when the window
/// holds no finite sample — the honest absence the stats panel renders as no
/// row, never a fabricated "0 °C" trend.
fn temperature_trend_stat_row(samples: &[f32]) -> Option<StatRow> {
    let raw_full = temperature_trend_value(samples)?;
    let mut latest = f32::NAN;
    let mut peak = f32::NAN;
    let mut sum = 0.0_f32;
    let mut count = 0_u32;
    for &value in samples.iter().filter(|value| value.is_finite()) {
        latest = value;
        peak = peak.max(value);
        sum += value;
        count += 1;
    }
    let avg = sum / count as f32;
    let latest_str = format!("{} {:.0} \u{b0}C", i18n::t("common.latest"), latest);
    let avg_str = format!("{} {:.0} \u{b0}C", i18n::t("common.avg"), avg);
    let peak_str = format!("{} {:.0} \u{b0}C", i18n::t("common.peak"), peak);
    Some(StatRow::trend(
        i18n::t("proc.trend"),
        latest_str,
        avg_str,
        peak_str,
        raw_full,
    ))
}

fn temperature_trend_value(samples: &[f32]) -> Option<String> {
    let mut latest = f32::NAN;
    let mut peak = f32::NAN;
    let mut sum = 0.0_f32;
    let mut count = 0_u32;
    for &value in samples.iter().filter(|value| value.is_finite()) {
        latest = value;
        peak = peak.max(value);
        sum += value;
        count += 1;
    }
    if count == 0 {
        return None;
    }
    Some(format!(
        "{} {:.0} \u{b0}C · {} {:.0} \u{b0}C · {} {:.0} \u{b0}C",
        i18n::t("common.latest"),
        latest,
        i18n::t("common.avg"),
        sum / count as f32,
        i18n::t("common.peak"),
        peak,
    ))
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_perf_views_disk_stats_tests.rs"]
mod tests;
