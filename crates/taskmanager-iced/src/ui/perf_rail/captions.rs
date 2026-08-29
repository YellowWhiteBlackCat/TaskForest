//! Pure text projections for Performance rail cards.

use taskmanager_application::i18n::t;
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::core::metrics::{
    CpuMetrics, DiskMetrics, GpuMetrics, MemoryMetrics, NetworkAdapterType, NetworkMetrics,
};
use taskmanager_core::core::power::BatteryInfo;
use taskmanager_core::core::sensors::SensorReading;

use taskmanager_shell::presentation::{
    device_status_i18n_key, effective_smart_status, gpu_display_identity, missing_value,
    smart_section_visible,
};

use super::super::UnitPrefs;
use super::super::perf_devices::{gpu_percent_readout, rate_text_pref};

/// Format a percentage observation honestly: unavailable renders "—", a
/// measured zero stays "0%". Shared by the CPU/disk caption lines.
fn honest_pct(value: Option<f32>) -> String {
    value.map_or_else(missing_value, |value| format!("{:.0}%", value.round()))
}

/// Append the localized status badge for a non-healthy device to a caption
/// line (healthy devices add nothing — they are the silent majority).
fn append_status_badge(caption: &mut String, status: DeviceStatus) {
    if status == DeviceStatus::Healthy {
        return;
    }
    if !caption.is_empty() {
        caption.push_str(" · ");
    }
    caption.push_str(t(device_status_i18n_key(status)));
}

pub(crate) fn cpu_rail_heading(cpu: &CpuMetrics) -> String {
    let _ = cpu;
    t("common.cpu").to_string()
}

/// CPU rail subtitle: the vendor brand/model string when the native source
/// exposes one; otherwise empty (the generic "CPU" heading stands alone).
#[must_use]
pub(crate) fn cpu_rail_subtitle(cpu: &CpuMetrics) -> String {
    cpu.brand
        .as_deref()
        .map(str::trim)
        .filter(|brand| !brand.is_empty())
        .map_or_else(String::new, str::to_string)
}

/// CPU caption lines: line 1 carries the headline usage (+ package
/// temperature when observed); line 2 carries the per-core min..max
/// temperature hint when the native provider exposes indexed readings.
pub(crate) fn cpu_rail_caption(cpu: &CpuMetrics) -> (String, String) {
    let mut cap1 = honest_pct(cpu.current_global_usage_pct());
    if let Some(temp) = cpu.current_temperature_c() {
        cap1.push_str(&format!(" · {:.0} \u{b0}C", temp.round()));
    }
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for index in 0..cpu.current_core_temperature_len() {
        if let Some(temp) = cpu.current_core_temperature_c(index) {
            min = min.min(temp);
            max = max.max(temp);
        }
    }
    let cap2 = if !min.is_finite() {
        String::new()
    } else if (max - min).abs() < 0.5 {
        t("sidebar.cpu_cores").replacen("{}", &format!("{:.0}", min.round()), 1)
    } else {
        t("sidebar.cpu_cores_range")
            .replacen("{}", &format!("{:.0}", min.round()), 1)
            .replacen("{}", &format!("{:.0}", max.round()), 1)
    };
    (cap1, cap2)
}

/// Memory caption lines: `used / total` on the resolved unit ladder, then the
/// used percentage.
pub(crate) fn mem_rail_caption(memory: &MemoryMetrics, units: UnitPrefs) -> (String, String) {
    let used = memory
        .current_used_bytes()
        .map_or_else(missing_value, |value| {
            super::super::memory_text_pref(value, units.use_bytes, units.use_base2)
        });
    let total = memory
        .current_total_bytes()
        .map_or_else(missing_value, |value| {
            super::super::memory_text_pref(value, units.use_bytes, units.use_base2)
        });
    (
        format!("{used} / {total}"),
        memory
            .used_percentage_observed()
            .map_or_else(missing_value, |value| format!("{value:.0}%")),
    )
}

fn clean_disk_name(name: &str) -> String {
    name.trim_start_matches("/dev/").to_string()
}

/// Rail heading: the neutral drive label plus the device node.
pub(crate) fn disk_rail_heading(disk: &DiskMetrics) -> String {
    format!("{} ({})", t("sidebar.drive"), clean_disk_name(&disk.name))
}

/// Disk rail subtitle: the vendor+model string when sysfs exposes one.
#[must_use]
pub(crate) fn disk_rail_subtitle(disk: &DiskMetrics) -> String {
    disk.model.trim().to_string()
}

/// Disk caption lines: active-time + throughput, then identity and SMART.
pub(crate) fn disk_rail_caption(disk: &DiskMetrics, units: UnitPrefs) -> (String, String) {
    let active = honest_pct(disk.current_active_time_pct());
    let rate = match (
        disk.current_read_bytes_per_sec(),
        disk.current_write_bytes_per_sec(),
    ) {
        (Some(read), Some(write)) => format!(
            "{}/s",
            super::super::quantity_text_pref(
                read.saturating_add(write),
                units.use_bytes,
                units.use_base2
            )
        ),
        _ => missing_value(),
    };
    let cap1 = format!("{active} · {rate}");
    let mut parts = vec![clean_disk_name(&disk.name)];
    if !disk.disk_type.trim().is_empty() {
        parts.push(disk.disk_type.trim().to_string());
    }
    if let Some(temp) = disk.smart_temperature_c
        && temp > 0.0
    {
        parts.push(format!("{:.0} \u{b0}C", temp.round()));
    }
    let mut cap2 = parts.join(" · ");
    if smart_section_visible(disk) {
        append_status_badge(&mut cap2, effective_smart_status(disk));
    }
    (cap1, cap2)
}

/// Localized adapter-category label shared by the rail and detail view.
pub(crate) fn network_category_label(adapter_type: NetworkAdapterType) -> String {
    match adapter_type {
        NetworkAdapterType::Ethernet => t("settings.network_wired"),
        NetworkAdapterType::WiFi => t("settings.network_wireless"),
        NetworkAdapterType::Vpn => t("settings.network_vpn"),
        NetworkAdapterType::Virtual => t("settings.network_virtual"),
        NetworkAdapterType::Unknown | NetworkAdapterType::Loopback | NetworkAdapterType::Other => {
            t("settings.network_other")
        }
    }
    .to_string()
}

/// Rail heading: category plus interface name.
pub(crate) fn nic_rail_heading(nic: &NetworkMetrics) -> String {
    format!(
        "{} ({})",
        network_category_label(nic.adapter_type()),
        nic.interface_name
    )
}

/// NIC rail subtitle: the physical adapter model when available.
#[must_use]
pub(crate) fn nic_rail_subtitle(nic: &NetworkMetrics) -> String {
    nic.adapter
        .as_deref()
        .map(str::trim)
        .filter(|adapter| !adapter.is_empty())
        .map_or_else(String::new, str::to_string)
}

/// NIC caption lines: live send/receive rates and association facts.
pub(crate) fn nic_rail_caption(nic: &NetworkMetrics, units: UnitPrefs) -> (String, String) {
    let tx = rate_text_pref(
        nic.current_tx_bytes_per_sec(),
        units.use_bytes,
        units.use_base2,
    );
    let rx = rate_text_pref(
        nic.current_rx_bytes_per_sec(),
        units.use_bytes,
        units.use_base2,
    );
    let cap1 = format!(
        "{} {} · {} {}",
        t("sidebar.send_label"),
        tx,
        t("sidebar.recv_label"),
        rx
    );
    let mut cap2 = nic_caption_line2(nic);
    append_status_badge(&mut cap2, nic.device_state.status);
    (cap1, cap2)
}

fn nic_caption_line2(nic: &NetworkMetrics) -> String {
    let mut parts: Vec<String> = Vec::new();
    if nic.adapter_type() == NetworkAdapterType::WiFi {
        if let Some(ssid) = nic.current_ssid()
            && !ssid.is_empty()
        {
            parts.push(ssid.to_string());
        }
        if let Some(signal) = nic.current_signal_dbm() {
            let pct = super::super::perf_devices::network::wifi_signal_quality_percent(signal);
            parts.push(format!("{pct:.0}%"));
        }
    } else {
        parts.push(nic.interface_name.as_ref().to_owned());
    }
    if let Some(link) = nic.current_link_speed_mbps() {
        parts.push(format!("{link} Mbps"));
    }
    if parts.is_empty() {
        nic.ipv4_addr
            .as_deref()
            .map(str::to_owned)
            .unwrap_or_else(|| nic.interface_name.as_ref().to_owned())
    } else {
        parts.join(" · ")
    }
}

/// Rail heading: the positional GPU label.
pub(crate) fn gpu_rail_heading(gpu: &GpuMetrics, index: usize) -> String {
    let _ = gpu;
    format!("{} {index}", t("common.gpu"))
}

/// GPU rail subtitle: the most specific resolved product identity.
#[must_use]
pub(crate) fn gpu_rail_subtitle(gpu: &GpuMetrics) -> String {
    gpu_display_identity(gpu)
        .headline
        .unwrap_or_default()
        .to_owned()
}

/// GPU caption lines: VRAM/clock, then utilization and optional telemetry.
pub(crate) fn gpu_rail_caption(gpu: &GpuMetrics, units: UnitPrefs) -> (String, String) {
    let (vram_used, vram_total) = (
        gpu.current_dedicated_vram_used_bytes(),
        gpu.current_dedicated_vram_total_bytes(),
    );
    let cap1 = match (vram_used, vram_total) {
        (Some(used), Some(total)) if total > 0 => format!(
            "VRAM {} / {}",
            super::super::memory_text_pref(used, units.use_bytes, units.use_base2),
            super::super::memory_text_pref(total, units.use_bytes, units.use_base2)
        ),
        _ => match (gpu.current_frequency_mhz(), gpu.current_max_frequency_mhz()) {
            (Some(current), Some(max)) if max > 0 => format!("{current} / {max} MHz"),
            (Some(current), _) => format!("{current} MHz"),
            _ => String::new(),
        },
    };
    let mut parts = vec![gpu_percent_readout(gpu.current_utilization_pct())];
    if let (Some(used), Some(total)) = (vram_used, vram_total)
        && total > 0
    {
        parts.push(format!(
            "VRAM {:.0}%",
            (used as f64 * 100.0 / total as f64).round()
        ));
    }
    if let Some(temp) = gpu.current_temperature_c() {
        parts.push(format!("{:.0} \u{b0}C", temp.round()));
    }
    if let Some(watts) = gpu.current_power_w()
        && watts > 0.0
    {
        parts.push(format!("{watts:.0} W"));
    }
    (cap1, parts.join(" · "))
}

/// Rail heading: the positional battery label.
pub(crate) fn battery_rail_heading(battery: &BatteryInfo, index: usize) -> String {
    let _ = battery;
    format!("{} {index}", t("common.battery"))
}

/// Battery rail subtitle: model, display name, or empty.
#[must_use]
pub(crate) fn battery_rail_subtitle(battery: &BatteryInfo) -> String {
    if !battery.model_name.trim().is_empty() {
        battery.model_name.trim().to_string()
    } else if !battery.display_name.trim().is_empty() {
        battery.display_name.trim().to_string()
    } else {
        String::new()
    }
}

/// Battery caption lines: current capacity and status.
pub(crate) fn battery_rail_caption(battery: &BatteryInfo) -> (String, String) {
    let cap1 = battery
        .current_capacity_pct()
        .map_or_else(missing_value, |value| format!("{value}%"));
    let mut cap2 = battery.status.clone();
    append_status_badge(&mut cap2, battery.device_state.status);
    (cap1, cap2)
}

pub(crate) fn fan_rail_heading(index: usize) -> String {
    format!("{} {index}", t("common.fan"))
}

pub(crate) fn fan_rail_subtitle(reading: &SensorReading) -> String {
    reading.label().to_owned()
}

/// Fan caption lines: current RPM and channel label.
pub(crate) fn fan_rail_caption(reading: &SensorReading) -> (String, String) {
    let rpm = reading
        .current_number()
        .map_or_else(missing_value, |value| format!("{value:.0} RPM"));
    let mut cap2 = reading.label().to_owned();
    append_status_badge(&mut cap2, reading.state().status);
    (rpm, cap2)
}
