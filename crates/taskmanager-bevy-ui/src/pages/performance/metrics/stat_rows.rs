//! Device header view models and per-device statistics rows for the
//! Performance page: one already-folded row list per target, with honest
//! dashes for observations the projection does not carry.

use super::*;
use taskforest_core::core::percent::percent_text;

/// One already-folded row for a renderer's statistics list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StatsRow {
    pub(crate) label: String,
    pub(crate) value: String,
}

impl StatsRow {
    fn new(label: String, value: String) -> Self {
        Self { label, value }
    }
}

/// Display title/subtitle/vital for a device header; a renderer paints these
/// strings directly instead of reading observations at scene-build time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HeaderViewModel {
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) vital: String,
}

pub(crate) fn disk_sidebar_title(disk: &DiskMetrics) -> String {
    if !disk.model.is_empty() {
        disk.model.clone()
    } else if !disk.name.is_empty() {
        disk.name.clone()
    } else {
        disk.device_id.clone()
    }
}

pub(crate) fn gpu_block_title(gpu: &GpuMetrics) -> String {
    let identity = gpu_display_identity(gpu);
    match (identity.headline, identity.qualifier) {
        (Some(headline), Some(qualifier)) => format!("{headline} ({qualifier})"),
        (Some(headline), None) => headline.to_owned(),
        (None, Some(qualifier)) => qualifier.to_owned(),
        (None, None) => gpu.device_id.clone(),
    }
}

pub(crate) fn memory_header_view_model(shell: &ShellApp) -> HeaderViewModel {
    let vital = memory_metrics(shell)
        .and_then(|memory| memory.current_total_bytes())
        .map(|bytes_value| format!("{} {}", t("mem.total"), bytes(bytes_value)))
        .unwrap_or_else(missing_value);
    HeaderViewModel {
        title: t("common.memory").to_owned(),
        vital,
        ..HeaderViewModel::default()
    }
}

pub(crate) fn disk_header_view_model(shell: &ShellApp) -> HeaderViewModel {
    let disk = disks(shell).and_then(|devices| devices.first());
    let title = disk
        .map(disk_sidebar_title)
        .unwrap_or_else(|| t("sidebar.disks").to_owned());
    let subtitle = disk
        .map(|disk| {
            format!(
                "{} · {} · {}",
                disk.name.trim_start_matches("/dev/"),
                if disk.disk_type.is_empty() {
                    missing_value()
                } else {
                    disk.disk_type.clone()
                },
                if disk.fs_type.is_empty() {
                    missing_value()
                } else {
                    disk.fs_type.clone()
                },
            )
        })
        .unwrap_or_default();
    let vital = disk
        .map(|disk| {
            match (
                disk.current_capacity_bytes(),
                disk.current_available_bytes(),
            ) {
                (Some(total), Some(free)) if total > 0 => {
                    let used = total.saturating_sub(free).min(total);
                    format!(
                        "{} / {} · {} {}",
                        bytes(used),
                        bytes(total),
                        disk.partitions.len(),
                        t("disk.partitions")
                    )
                }
                _ => missing_value(),
            }
        })
        .unwrap_or_default();
    HeaderViewModel {
        title,
        subtitle,
        vital,
    }
}

pub(crate) fn network_header_view_model(shell: &ShellApp) -> HeaderViewModel {
    let nic = network_devices(shell).and_then(|devices| devices.first());
    let is_wireless = nic
        .map(|nic| nic.adapter_type() == NetworkAdapterType::WiFi)
        .unwrap_or(false);
    let title = nic
        .map(|nic| {
            if is_wireless
                && let Some(ssid) = nic.current_ssid()
                && !ssid.is_empty()
            {
                format!("{}: {} ({})", t("sidebar.wifi"), ssid, nic.interface_name)
            } else if is_wireless {
                format!("{} ({})", t("sidebar.wireless"), nic.interface_name)
            } else {
                format!("{} ({})", t("net.wired"), nic.interface_name)
            }
        })
        .unwrap_or_else(|| t("sidebar.network").to_owned());
    let subtitle = nic
        .and_then(|nic| nic.ipv4_addr.as_ref().map(|ssid| ssid.to_string()))
        .unwrap_or_default();
    let vital = nic
        .map(|nic| {
            let status = if nic.current_link_up() == Some(true) {
                t("common.connected")
            } else {
                t("common.disconnected")
            };
            if let Some(speed) = nic.current_link_speed_mbps() {
                format!("{status} · {speed} Mbps")
            } else {
                status.to_owned()
            }
        })
        .unwrap_or_default();
    HeaderViewModel {
        title,
        subtitle,
        vital,
    }
}

pub(crate) fn gpu_header_view_model(shell: &ShellApp) -> HeaderViewModel {
    let gpu = gpu_devices(shell).and_then(|devices| devices.first());
    let title = gpu
        .map(gpu_block_title)
        .unwrap_or_else(|| t("common.gpu").to_owned());
    let subtitle = gpu.and_then(|gpu| gpu.driver.clone()).unwrap_or_default();
    let vital = gpu
        .and_then(|gpu| gpu.current_utilization_pct())
        .map(percent_text)
        .unwrap_or_default();
    HeaderViewModel {
        title,
        subtitle,
        vital,
    }
}

pub(crate) fn battery_header_view_model(shell: &ShellApp) -> HeaderViewModel {
    let battery = batteries(shell).and_then(|devices| devices.first());
    let title = battery
        .map(|battery| battery_sidebar_title(battery, 0))
        .unwrap_or_else(|| t("common.battery").to_owned());
    let subtitle = battery
        .map(|battery| battery.status.clone())
        .unwrap_or_default();
    let vital = battery
        .and_then(|battery| battery.current_capacity_pct())
        .map(percent_text)
        .unwrap_or_default();
    HeaderViewModel {
        title,
        subtitle,
        vital,
    }
}

pub(crate) fn fan_header_view_model(shell: &ShellApp) -> HeaderViewModel {
    let fan = fans(shell).and_then(|devices| devices.into_iter().next());
    let title = fan
        .as_ref()
        .map(|fan| {
            if fan.label().trim().is_empty() {
                format!("{} 0", t("common.fan"))
            } else {
                fan.label().trim().to_owned()
            }
        })
        .unwrap_or_else(|| t("common.fan").to_owned());
    let subtitle = fan
        .and_then(|fan| fan.current_number())
        .map(|rpm| format!("{rpm:.0} RPM"))
        .unwrap_or_default();
    HeaderViewModel {
        title,
        subtitle,
        ..HeaderViewModel::default()
    }
}

pub(crate) fn disk_stat_rows(shell: &ShellApp) -> Vec<StatsRow> {
    let Some(disk) = disks(shell).and_then(|devices| devices.first()) else {
        return Vec::new();
    };
    let mut rows = vec![StatsRow::new(
        t("device.status").to_owned(),
        t(device_status_i18n_key(disk.device_state.status)).to_owned(),
    )];
    let optional =
        |label: String, value: Option<String>| value.map(|value| StatsRow::new(label, value));
    rows.extend(optional(
        t("disk.active_time").to_owned(),
        disk.current_active_time_pct().map(percent_text),
    ));
    rows.extend(optional(
        t("disk.read").to_owned(),
        disk.current_read_bytes_per_sec()
            .map(|value| format!("{}/s", bytes(value))),
    ));
    rows.extend(optional(
        t("disk.write").to_owned(),
        disk.current_write_bytes_per_sec()
            .map(|value| format!("{}/s", bytes(value))),
    ));
    rows.extend(optional(
        t("disk.iops").to_owned(),
        disk.current_iops().map(|value| value.to_string()),
    ));
    rows.extend(optional(
        t("disk.response").to_owned(),
        disk.current_response_time_ms()
            .map(|value| format!("{value:.2} ms")),
    ));
    rows.extend(optional(
        t("disk.queue_depth").to_owned(),
        disk.current_average_queue_depth()
            .map(|value| format!("{value:.2}")),
    ));
    rows.extend(optional(
        t("disk.service_time").to_owned(),
        disk.current_service_time_ms()
            .map(|value| format!("{value:.2} ms")),
    ));
    rows.extend(optional(
        t("disk.capacity").to_owned(),
        disk.current_capacity_bytes().map(bytes),
    ));
    rows.extend(optional(
        t("disk.free").to_owned(),
        disk.current_available_bytes().map(bytes),
    ));
    if !disk.disk_type.trim().is_empty() {
        rows.push(StatsRow::new(
            t("common.type").to_owned(),
            disk.disk_type.trim().to_owned(),
        ));
    }
    if !disk.fs_type.trim().is_empty() {
        rows.push(StatsRow::new(
            t("disk.filesystem").to_owned(),
            disk.fs_type.trim().to_owned(),
        ));
    }
    if let Some(serial) = disk.serial.as_deref().filter(|value| !value.is_empty()) {
        rows.push(StatsRow::new(
            t("disk.serial").to_owned(),
            serial.to_owned(),
        ));
    }
    if let Some(temperature) = disk.smart_temperature_c {
        rows.push(StatsRow::new(
            t("common.temperature").to_owned(),
            format!("{temperature:.0} °C"),
        ));
    }
    if let Some(value) = disk.smart_available_spare_pct {
        rows.push(StatsRow::new(
            t("disk.available_spare").to_owned(),
            percent_text(value),
        ));
    }
    if let Some(value) = disk.smart_percent_used {
        rows.push(StatsRow::new(
            t("disk.endurance_used").to_owned(),
            percent_text(value),
        ));
    }
    if let Some(hours) = disk.smart_power_on_hours {
        let days = hours / 24;
        rows.push(StatsRow::new(
            t("disk.power_on").to_owned(),
            format!("{hours}h ({days}d)"),
        ));
    }
    rows.push(StatsRow::new(
        t("disk.smart_status").to_owned(),
        t(device_status_i18n_key(effective_smart_status(disk))).to_owned(),
    ));
    rows
}

pub(crate) fn network_stat_rows(shell: &ShellApp) -> Vec<StatsRow> {
    let Some(nic) = network_devices(shell).and_then(|devices| devices.first()) else {
        return Vec::new();
    };
    let mut rows = vec![StatsRow::new(
        t("device.status").to_owned(),
        t(device_status_i18n_key(nic.device_state.status)).to_owned(),
    )];
    let optional =
        |label: String, value: Option<String>| value.map(|value| StatsRow::new(label, value));
    rows.extend(optional(
        t("net.receive").to_owned(),
        nic.current_rx_bytes_per_sec()
            .map(|value| format!("{}/s", bytes(value))),
    ));
    rows.extend(optional(
        t("net.send").to_owned(),
        nic.current_tx_bytes_per_sec()
            .map(|value| format!("{}/s", bytes(value))),
    ));
    rows.extend(optional(
        t("net.connection").to_owned(),
        nic.current_link_up().map(|up| {
            if up {
                t("common.connected")
            } else {
                t("common.disconnected")
            }
            .to_owned()
        }),
    ));
    rows.extend(optional(
        t("net.total_received").to_owned(),
        nic.current_total_rx_bytes().map(bytes),
    ));
    rows.extend(optional(
        t("net.total_sent").to_owned(),
        nic.current_total_tx_bytes().map(bytes),
    ));
    let is_wireless = nic.adapter_type() == NetworkAdapterType::WiFi;
    rows.push(StatsRow::new(
        t("common.type").to_owned(),
        if is_wireless {
            t("sidebar.wireless")
        } else {
            t("net.wired")
        }
        .to_owned(),
    ));
    if let Some(value) = nic.ipv4_addr.as_deref().filter(|value| !value.is_empty()) {
        rows.push(StatsRow::new(t("net.ipv4").to_owned(), value.to_owned()));
    }
    if let Some(value) = nic.ipv6_addr.as_deref().filter(|value| !value.is_empty()) {
        rows.push(StatsRow::new(t("net.ipv6").to_owned(), value.to_owned()));
    }
    if let Some(value) = nic.mac_addr.as_deref().filter(|value| !value.is_empty()) {
        rows.push(StatsRow::new(t("net.mac").to_owned(), value.to_owned()));
    }
    rows.extend(optional(
        t("net.link").to_owned(),
        nic.current_link_speed_mbps()
            .map(|value| format!("{value} Mbps")),
    ));
    rows.extend(optional(
        t("common.utilization").to_owned(),
        nic.current_utilization_pct().map(percent_text),
    ));
    if let Some(value) = nic.driver.as_deref().filter(|value| !value.is_empty()) {
        rows.push(StatsRow::new(
            t("common.driver").to_owned(),
            value.to_owned(),
        ));
    }
    if is_wireless && let Some(signal) = nic.current_signal_dbm() {
        let quality = wifi_signal_quality_percent(signal);
        rows.push(StatsRow::new(
            t("common.signal").to_owned(),
            format!("{signal} dBm ({})", percent_text(quality)),
        ));
    }
    rows
}

pub(crate) fn gpu_stat_rows(shell: &ShellApp) -> Vec<StatsRow> {
    let Some(gpu) = gpu_devices(shell).and_then(|devices| devices.first()) else {
        return Vec::new();
    };
    let mut rows = vec![StatsRow::new(
        t("device.status").to_owned(),
        t(device_status_i18n_key(gpu.device_state.status)).to_owned(),
    )];
    let optional =
        |label: String, value: Option<String>| value.map(|value| StatsRow::new(label, value));
    rows.extend(optional(
        t("common.utilization").to_owned(),
        gpu.current_utilization_pct().map(percent_text),
    ));
    if let Some(value) = gpu
        .marketing_name
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        rows.push(StatsRow::new(
            t("gpu.marketing_name").to_owned(),
            value.to_owned(),
        ));
    }
    rows.extend(optional(
        t("common.clock").to_owned(),
        gpu.current_frequency_mhz()
            .map(|value| format!("{value} MHz")),
    ));
    rows.extend(optional(
        t("gpu.max_clock").to_owned(),
        gpu.current_max_frequency_mhz()
            .map(|value| format!("{value} MHz")),
    ));
    rows.extend(optional(
        t("common.power").to_owned(),
        gpu.current_power_w().map(|value| format!("{value:.1} W")),
    ));
    rows.extend(optional(
        t("common.temperature").to_owned(),
        gpu.current_temperature_c()
            .map(|value| format!("{value:.0} °C")),
    ));
    let vram_pair = |label: &str, used: Option<u64>, total: Option<u64>| {
        match (used, total) {
            (Some(used), Some(total)) if total > 0 => Some((used.min(total), total)),
            _ => None,
        }
        .map(|(used, total)| {
            StatsRow::new(
                label.to_owned(),
                format!("{} / {}", bytes(used), bytes(total)),
            )
        })
    };
    if let Some(row) = vram_pair(
        "gpu.dedicated_vram",
        gpu.current_dedicated_vram_used_bytes(),
        gpu.current_dedicated_vram_total_bytes(),
    ) {
        rows.push(row);
    }
    if let Some(row) = vram_pair(
        "gpu.shared_vram",
        gpu.current_shared_vram_used_bytes(),
        gpu.current_shared_vram_total_bytes(),
    ) {
        rows.push(row);
    }
    if let Some(row) = vram_pair(
        "gpu.vram",
        gpu.current_memory_used_bytes(),
        gpu.current_memory_total_bytes(),
    ) {
        rows.push(row);
    }
    if let Some(value) = gpu.driver.as_deref().filter(|value| !value.is_empty()) {
        rows.push(StatsRow::new(
            t("common.driver").to_owned(),
            value.to_owned(),
        ));
    }
    if let Some(value) = gpu
        .driver_version
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        rows.push(StatsRow::new(
            t("gpu.driver_version").to_owned(),
            value.to_owned(),
        ));
    }
    if let Some(display) = gpu.display_connected {
        rows.push(StatsRow::new(
            t("gpu.display_output").to_owned(),
            if display {
                t("common.yes")
            } else {
                t("common.no")
            }
            .to_owned(),
        ));
    }
    rows
}

pub(crate) fn battery_stat_rows(shell: &ShellApp) -> Vec<StatsRow> {
    let Some(battery) = batteries(shell).and_then(|devices| devices.first()) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    if let Some(capacity) = battery.current_capacity_pct() {
        rows.push(StatsRow::new(
            t("battery.capacity").to_owned(),
            percent_text(capacity),
        ));
    }
    rows.push(StatsRow::new(
        t("battery.status").to_owned(),
        battery.status.clone(),
    ));
    let optional =
        |label: String, value: Option<String>| value.map(|value| StatsRow::new(label, value));
    rows.extend(optional(
        t("battery.power").to_owned(),
        battery
            .current_power_w()
            .map(|value| format!("{value:.1} W")),
    ));
    rows.extend(optional(
        t("battery.voltage").to_owned(),
        battery
            .current_voltage_uv()
            .map(|value| format!("{:.2} V", value as f64 / 1_000_000.0)),
    ));
    rows.extend(optional(
        t("battery.cycles").to_owned(),
        battery.current_cycle_count().map(|value| value.to_string()),
    ));
    rows.extend(optional(
        t("battery.health").to_owned(),
        battery.current_health_pct().map(percent_text),
    ));
    rows.extend(optional(
        t("battery.time_to_full").to_owned(),
        battery
            .current_time_to_full_secs()
            .map(|value| duration(value as u64)),
    ));
    rows.extend(optional(
        t("battery.time_to_empty").to_owned(),
        battery
            .current_time_to_empty_secs()
            .map(|value| duration(value as u64)),
    ));
    if !battery.technology.is_empty() {
        rows.push(StatsRow::new(
            t("battery.technology").to_owned(),
            battery.technology.clone(),
        ));
    }
    rows
}

pub(crate) fn fan_stat_rows(shell: &ShellApp) -> Vec<StatsRow> {
    let Some(fan) = fans(shell).and_then(|devices| devices.into_iter().next()) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    if let Some(rpm) = fan.current_number() {
        rows.push(StatsRow::new(
            t("fan.rpm").to_owned(),
            format!("{rpm:.0} RPM"),
        ));
    }
    if let Some(sensors) = shell.projection().sensors.as_ref() {
        for reading in sensors.readings.iter().filter(|reading| {
            reading.device_id() == fan.device_id()
                && reading.quantity() == &SensorQuantity::Temperature
        }) {
            if let Some(value) = reading.current_number() {
                rows.push(StatsRow::new(
                    format!("{} {}", t("common.temperature"), reading.label()),
                    format!("{value:.1} °C"),
                ));
            }
        }
    }
    rows
}

pub(crate) fn memory_section_summary(shell: &ShellApp) -> Option<String> {
    let memory = memory_metrics(shell)?;
    let used = memory.current_used_bytes()?;
    let total = memory.current_total_bytes()?;
    Some(format!(
        "{} {} · {} {}",
        t("mem.in_use"),
        bytes(used),
        bytes(total),
        t("mem.total")
    ))
}

pub(crate) fn memory_segments_total(shell: &ShellApp) -> Option<u64> {
    memory_metrics(shell).and_then(|memory| memory.current_total_bytes().filter(|total| *total > 0))
}

pub(crate) fn swap_bar_fractions(shell: &ShellApp) -> Option<(f32, f32)> {
    let memory = memory_metrics(shell)?;
    let used = memory.current_swap_used_bytes()?;
    let total = memory
        .current_swap_total_bytes()
        .filter(|total| *total > 0)?;
    let used_fraction = (used as f64 / total as f64).clamp(0.0, 1.0) as f32;
    Some((used_fraction, 1.0 - used_fraction))
}
