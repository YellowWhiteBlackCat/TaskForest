//! Sidebar caption projections kept separate from the device-row renderer.

use crate::core::device_state::DeviceStatus;
use crate::core::metrics::{DiskMetrics, GpuMetrics, NetworkMetrics, SystemSnapshot};
use crate::core::{BatteryInfo, SensorReading};
use crate::gpui_app::formatting::{self, DisplayUnits, UnitKind};
use crate::gpui_app::perf_views::gpu_percentage_readout;
use crate::i18n;

pub(super) fn rate_str(units: DisplayUnits, bytes_per_sec: u64) -> String {
    units.format(bytes_per_sec, UnitKind::Network, true)
}

pub(super) fn clean_disk_name(name: &str) -> String {
    name.trim_start_matches("/dev/")
        .trim_end_matches('\\')
        .to_string()
}

pub(super) fn disk_activity_caption(disk: &DiskMetrics, units: DisplayUnits) -> String {
    let active = disk
        .current_active_time_pct()
        .map_or_else(formatting::missing_value, |value| {
            format!("{:.0}%", value.round())
        });
    let rate = match (
        disk.current_read_bytes_per_sec(),
        disk.current_write_bytes_per_sec(),
    ) {
        (Some(read), Some(write)) => {
            units.format(read.saturating_add(write), UnitKind::Drive, true)
        }
        _ => formatting::missing_value(),
    };
    format!("{active}  ·  {rate}")
}

pub(super) fn network_rate_caption(network: &NetworkMetrics, units: DisplayUnits) -> String {
    let tx = network
        .current_tx_bytes_per_sec()
        .map_or_else(formatting::missing_value, |value| rate_str(units, value));
    let rx = network
        .current_rx_bytes_per_sec()
        .map_or_else(formatting::missing_value, |value| rate_str(units, value));
    format!(
        "{} {}  {} {}",
        i18n::t("sidebar.send_label"),
        tx,
        i18n::t("sidebar.recv_label"),
        rx
    )
}

pub(super) fn battery_capacity_caption(battery: &BatteryInfo) -> String {
    battery
        .current_capacity_pct()
        .map_or_else(formatting::missing_value, |value| format!("{value}%"))
}

pub(super) fn fan_speed_caption(reading: &SensorReading) -> String {
    reading
        .current_number()
        .map_or_else(formatting::missing_value, |value| format!("{value:.0} RPM"))
}

pub(super) fn cpu_caption(snap: &SystemSnapshot) -> (String, String) {
    let cpu = &snap.cpu;
    let brand = cpu
        .brand
        .as_deref()
        .map(str::trim)
        .filter(|brand| !brand.is_empty())
        .map_or_else(formatting::missing_value, str::to_string);
    let usage = cpu
        .current_global_usage_pct()
        .map_or_else(formatting::missing_value, |value| {
            format!("{:.0}%", value.round())
        });

    let freq_str = cpu.current_frequency_mhz().map(|mhz| {
        if mhz >= 1000 {
            formatting::format_ghz(mhz)
        } else {
            format!("{mhz} MHz")
        }
    });

    let mut line2 = match (cpu.current_temperature_c(), freq_str) {
        (Some(temp), Some(freq)) => format!("{usage}  {freq} ({:.0} °C)", temp.round()),
        (Some(temp), None) => format!("{usage} ({:.0} °C)", temp.round()),
        (None, Some(freq)) => format!("{usage}  {freq}"),
        (None, None) => usage,
    };
    // Per-core min..max hint when the native provider exposes indexed
    // temperatures. With no mapped readings nothing is appended.
    // The Linux provider emits typed `None` for unmapped logical cores
    // (topology-mapped SMT siblings still carry their parent physical core's
    // reading), so the legacy `>0.0` sentinel is no longer needed. cap2 is
    // truncate-safe so the longer string can't overflow the row; the
    // label-width invariant lives in `device_row`'s structure (unchanged here
    // — only the caption STRING grows).
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for index in 0..cpu.current_core_temperature_len() {
        if let Some(t) = cpu.current_core_temperature_c(index) {
            min = min.min(t);
            max = max.max(t);
        }
    }
    if min.is_finite() {
        // Collapse min..max to a single value when every reporting core reads
        // the same temp (common at idle) — "cores 45 °C" beats "45..45 °C".
        if (max - min).abs() < 0.5 {
            line2.push_str(&format!("  ·  cores {:.0} °C", min.round()));
        } else {
            line2.push_str(&format!(
                "  ·  cores {:.0}..{:.0} °C",
                min.round(),
                max.round()
            ));
        }
    }
    (brand, line2)
}

pub(super) fn mem_caption(snap: &SystemSnapshot, units: DisplayUnits) -> (String, String) {
    let m = &snap.memory;
    let used = m
        .current_used_bytes()
        .map_or_else(formatting::missing_value, |value| {
            units.format(value, UnitKind::Memory, false)
        });
    let total = m
        .current_total_bytes()
        .map_or_else(formatting::missing_value, |value| {
            units.format(value, UnitKind::Memory, false)
        });
    (
        format!("{used} / {total}"),
        m.used_percentage_observed()
            .map_or_else(formatting::missing_value, |value| format!("{value:.0}%")),
    )
}

pub(super) fn append_status_badge(caption: &mut String, status: DeviceStatus) {
    if status == DeviceStatus::Healthy {
        return;
    }
    let key = crate::gpui_app::perf_views::device_status_i18n_key(status);
    if !caption.is_empty() {
        caption.push_str("  ·  ");
    }
    caption.push_str(i18n::t(key));
}

/// First GPU caption line: absolute memory when exposed, otherwise the live
/// clock. The clock is shown with the advertised max when available, e.g.
/// "1300 / 2500 MHz". Returns an empty string only when neither fact exists.
pub(super) fn gpu_caption_line1(g: &GpuMetrics, units: DisplayUnits) -> String {
    let (vram_used, vram_total) = if let (Some(used), Some(total)) = (
        g.current_dedicated_vram_used_bytes(),
        g.current_dedicated_vram_total_bytes(),
    ) && total > 512 * 1024 * 1024
    {
        (Some(used), Some(total))
    } else if let (Some(used), Some(total)) = (
        g.current_memory_used_bytes(),
        g.current_memory_total_bytes(),
    ) && total > 0
    {
        (Some(used), Some(total))
    } else if let (Some(used), Some(total)) = (
        g.current_dedicated_vram_used_bytes(),
        g.current_dedicated_vram_total_bytes(),
    ) && total > 0
    {
        (Some(used.min(total)), Some(total))
    } else {
        (None, None)
    };

    if let (Some(used), Some(total)) = (vram_used, vram_total)
        && total > 0
    {
        return format!(
            "VRAM {} / {}",
            units.format(used, UnitKind::Memory, false),
            units.format(total, UnitKind::Memory, false)
        );
    }
    if let Some(total) = vram_total
        && total > 0
    {
        return format!("VRAM {}", units.format(total, UnitKind::Memory, false));
    }
    // Fall back to the core clock or driver.
    match (g.current_frequency_mhz(), g.current_max_frequency_mhz()) {
        (Some(cur), Some(max)) if max > 0 => format!("{} / {} MHz", cur, max),
        (Some(cur), _) => format!("{} MHz", cur),
        _ => g
            .driver
            .as_deref()
            .map_or_else(String::new, |d| format!("Driver {d}")),
    }
}

/// Second GPU caption line: utilization · VRAM% · temp°C · power W. Unknown
/// utilization is rendered as an em dash; a measured zero remains `0%`.
/// Remaining pieces are gated on the corresponding optional source.
pub(super) fn gpu_caption_line2(g: &GpuMetrics) -> String {
    let mut parts = Vec::new();
    if let Some(util) = g.current_utilization_pct() {
        parts.push(format!("{util:.0}%"));
    }
    let (vram_used, vram_total) = if let (Some(used), Some(total)) = (
        g.current_dedicated_vram_used_bytes(),
        g.current_dedicated_vram_total_bytes(),
    ) && total > 512 * 1024 * 1024
    {
        (Some(used), Some(total))
    } else if let (Some(used), Some(total)) = (
        g.current_memory_used_bytes(),
        g.current_memory_total_bytes(),
    ) && total > 0
    {
        (Some(used), Some(total))
    } else if let (Some(used), Some(total)) = (
        g.current_dedicated_vram_used_bytes(),
        g.current_dedicated_vram_total_bytes(),
    ) && total > 0
    {
        (Some(used.min(total)), Some(total))
    } else {
        (None, None)
    };

    if let (Some(used), Some(total)) = (vram_used, vram_total)
        && total > 0
    {
        let pct = formatting::bytes_percent(used, total);
        parts.push(format!("VRAM {:.0}%", pct.min(100.0).round()));
    }
    if let Some(temp) = g.current_temperature_c() {
        parts.push(format!("{:.0} °C", temp.round()));
    }
    if let Some(p) = g.current_power_w()
        && p > 0.0
    {
        parts.push(format!("{:.0} W", p));
    }
    if parts.is_empty() {
        gpu_percentage_readout(g.current_utilization_pct())
    } else {
        parts.join("  ·  ")
    }
}
