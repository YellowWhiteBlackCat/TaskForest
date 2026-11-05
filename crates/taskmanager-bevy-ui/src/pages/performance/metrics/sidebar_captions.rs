//! Device sidebar caption formatting across performance targets.

use taskforest_core::core::device_state::DeviceStatus;
use taskforest_core::core::metrics::{DiskMetrics, GpuMetrics, NetworkAdapterType, NetworkMetrics};
use taskforest_core::core::power::BatteryInfo;
use taskforest_core::core::sensors::SensorReading;
use taskforest_core::core::units::{QuantityFamily, UnitPreferences, bytes_percent};
use taskmanager_application::i18n::t;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::{
    bytes, device_status_i18n_key, effective_smart_status, missing_value,
    optional_wifi_signal_quality_percent, smart::smart_section_visible,
};

use super::cpu_sidebar_caption;
use super::{batteries, disks, fans, gpu_devices, memory_metrics, network_devices};
use crate::pages::performance::PerformanceDeviceTarget;

pub(crate) fn append_status_badge(caption: &mut String, status: DeviceStatus) {
    if status == DeviceStatus::Healthy {
        return;
    }
    let key = device_status_i18n_key(status);
    if !caption.is_empty() {
        caption.push_str("  ·  ");
    }
    caption.push_str(t(key));
}

pub(crate) fn memory_sidebar_caption(shell: &ShellApp) -> (String, String) {
    if let Some(mem) = memory_metrics(shell) {
        let used = mem
            .current_used_bytes()
            .map(bytes)
            .unwrap_or_else(missing_value);
        let total = mem
            .current_total_bytes()
            .map(bytes)
            .unwrap_or_else(missing_value);
        let cap1 = format!("{used} / {total}");
        let cap2 = mem
            .used_percentage_observed()
            .filter(|value| value.is_finite())
            .map(|value| format!("{value:.0}%"))
            .unwrap_or_else(missing_value);
        (cap1, cap2)
    } else {
        (missing_value(), missing_value())
    }
}

pub(crate) fn disk_sidebar_caption(disk: &DiskMetrics) -> (String, String) {
    let units = UnitPreferences::default();
    let active = disk
        .current_active_time_pct()
        .map_or_else(missing_value, |value| format!("{value:.0}%"));
    let rate = match (
        disk.current_read_bytes_per_sec(),
        disk.current_write_bytes_per_sec(),
    ) {
        (Some(read), Some(write)) => {
            units.format_quantity(read.saturating_add(write), QuantityFamily::Drive, true)
        }
        _ => missing_value(),
    };
    let cap1 = format!("{active}  ·  {rate}");
    let base = disk.name.trim_start_matches("/dev/").to_owned();
    let mut cap2 = match disk.smart_temperature_c.filter(|value| *value > 0.0) {
        Some(temperature) => format!("{base}  ·  {}  ·  {temperature:.0} °C", disk.disk_type),
        None => format!("{base}  ·  {}", disk.disk_type),
    };
    if smart_section_visible(disk) {
        append_status_badge(&mut cap2, effective_smart_status(disk));
    }
    (cap1, cap2)
}

pub(crate) fn network_sidebar_caption(nic: &NetworkMetrics) -> (String, String) {
    let units = UnitPreferences::default();
    let rate = |value: Option<u64>| {
        value.map_or_else(missing_value, |value| {
            units.format_quantity(value, QuantityFamily::Network, true)
        })
    };
    let cap1 = format!(
        "{} {}  {} {}",
        t("sidebar.send_label"),
        rate(nic.current_tx_bytes_per_sec()),
        t("sidebar.recv_label"),
        rate(nic.current_rx_bytes_per_sec()),
    );
    let mut parts: Vec<String> = Vec::new();
    if nic.adapter_type() == NetworkAdapterType::WiFi {
        if let Some(ssid) = nic.current_ssid()
            && !ssid.is_empty()
        {
            parts.push(ssid.to_string());
        }
        if let Some(pct) = optional_wifi_signal_quality_percent(nic.current_signal_dbm()) {
            parts.push(format!("{pct:.0}%"));
        }
    } else {
        parts.push((*nic.interface_name).to_owned());
    }
    if let Some(link) = nic.current_link_speed_mbps() {
        parts.push(format!("{link} Mbps"));
    }
    let mut cap2 = if parts.is_empty() {
        nic.ipv4_addr
            .as_deref()
            .map(str::to_owned)
            .unwrap_or_else(|| (*nic.interface_name).to_owned())
    } else {
        parts.join("  ·  ")
    };
    append_status_badge(&mut cap2, nic.device_state.status);
    (cap1, cap2)
}

pub(crate) fn gpu_sidebar_caption(gpu: &GpuMetrics) -> (String, String) {
    let units = UnitPreferences::default();
    let vram = |gpu: &GpuMetrics| -> (Option<u64>, Option<u64>) {
        if let (Some(used), Some(total)) = (
            gpu.current_dedicated_vram_used_bytes(),
            gpu.current_dedicated_vram_total_bytes(),
        ) && total > 512 * 1024 * 1024
        {
            (Some(used), Some(total))
        } else if let (Some(used), Some(total)) = (
            gpu.current_memory_used_bytes(),
            gpu.current_memory_total_bytes(),
        ) && total > 0
        {
            (Some(used), Some(total))
        } else if let (Some(used), Some(total)) = (
            gpu.current_dedicated_vram_used_bytes(),
            gpu.current_dedicated_vram_total_bytes(),
        ) && total > 0
        {
            (Some(used.min(total)), Some(total))
        } else {
            (None, None)
        }
    };
    let cap1 = match vram(gpu) {
        (Some(used), Some(total)) if total > 0 => format!(
            "VRAM {} / {}",
            units.format_quantity(used, QuantityFamily::Memory, false),
            units.format_quantity(total, QuantityFamily::Memory, false),
        ),
        (Some(_), Some(total)) if total > 0 => {
            format!(
                "VRAM {}",
                units.format_quantity(total, QuantityFamily::Memory, false)
            )
        }
        _ => match (gpu.current_frequency_mhz(), gpu.current_max_frequency_mhz()) {
            (Some(current), Some(max)) if max > 0 => format!("{current} / {max} MHz"),
            (Some(current), _) => format!("{current} MHz"),
            _ => gpu
                .driver
                .as_deref()
                .map_or_else(String::new, |driver| format!("Driver {driver}")),
        },
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(utilization) = gpu.current_utilization_pct() {
        parts.push(format!("{utilization:.0}%"));
    }
    if let (Some(used), Some(total)) = vram(gpu)
        && total > 0
        && let Some(pct) = bytes_percent(used, total)
    {
        parts.push(format!("VRAM {:.0}%", pct.min(100.0).round()));
    }
    if let Some(temperature) = gpu.current_temperature_c() {
        parts.push(format!("{temperature:.0} °C"));
    }
    if let Some(power) = gpu.current_power_w().filter(|value| *value > 0.0) {
        parts.push(format!("{power:.0} W"));
    }
    if parts.is_empty() {
        parts.push(missing_value());
    }
    let mut cap2 = parts.join("  ·  ");
    append_status_badge(&mut cap2, gpu.device_state.status);
    (cap1, cap2)
}

pub(crate) fn battery_sidebar_caption(battery: &BatteryInfo) -> (String, String) {
    let cap1 = battery
        .current_capacity_pct()
        .map_or_else(missing_value, |value| format!("{value}%"));
    let mut cap2 = battery.status.clone();
    append_status_badge(&mut cap2, battery.device_state.status);
    (cap1, cap2)
}

pub(crate) fn fan_sidebar_caption(reading: &SensorReading) -> (String, String) {
    let cap1 = reading
        .current_number()
        .map_or_else(missing_value, |value| format!("{value:.0} RPM"));
    let mut cap2 = reading.label().to_owned();
    append_status_badge(&mut cap2, reading.state().status);
    (cap1, cap2)
}

pub(crate) fn device_sidebar_caption(
    shell: &ShellApp,
    target: &PerformanceDeviceTarget,
) -> (String, String) {
    match target {
        PerformanceDeviceTarget::Cpu => cpu_sidebar_caption(shell),
        PerformanceDeviceTarget::Memory => memory_sidebar_caption(shell),
        PerformanceDeviceTarget::Disk(id) => disks(shell)
            .and_then(|disks| disks.iter().find(|disk| &disk.device_id == id))
            .map_or_else(|| (missing_value(), missing_value()), disk_sidebar_caption),
        PerformanceDeviceTarget::Network(id) => network_devices(shell)
            .and_then(|nics| nics.iter().find(|nic| nic.device_id.as_ref() == id))
            .map_or_else(
                || (missing_value(), missing_value()),
                network_sidebar_caption,
            ),
        PerformanceDeviceTarget::Gpu(id) => gpu_devices(shell)
            .and_then(|gpus| gpus.iter().find(|gpu| &gpu.device_id == id))
            .map_or_else(|| (missing_value(), missing_value()), gpu_sidebar_caption),
        PerformanceDeviceTarget::Battery(id) => batteries(shell)
            .and_then(|bats| bats.iter().find(|bat| &bat.id == id))
            .map_or_else(
                || (missing_value(), missing_value()),
                battery_sidebar_caption,
            ),
        PerformanceDeviceTarget::Fan(id) => fans(shell)
            .and_then(|fans| fans.into_iter().find(|fan| fan.id() == id))
            .map_or_else(|| (missing_value(), missing_value()), fan_sidebar_caption),
    }
}
