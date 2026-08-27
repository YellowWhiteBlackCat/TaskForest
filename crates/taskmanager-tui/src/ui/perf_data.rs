//! Pure observation folds for TUI Performance device surfaces.
//!
//! Ratatui renderers consume these grouped, display-ready projections. Typed
//! availability is resolved once here, so paint code never reads a live
//! `current_*` scalar or recreates dash/zero semantics per widget.

use taskmanager_application::{
    BatteryInfo, DirectoryUsageEntry, DirectoryUsageSnapshot, DiskMetrics, DiskPartition,
    GpuMetrics, MemoryMetrics, NetworkAdapterType, NetworkMetrics, i18n::t,
};
use taskmanager_shell::memory::{self, MemSegment, SwapBreakdown};
use taskmanager_shell::presentation::{MISSING_VALUE, missing_value, optional_bytes};

use super::units::{
    observed_frequency, observed_percentage, observed_temperature, quantity_text_optional,
};
use taskmanager_shell::presentation::duration;

pub(super) struct BatteryData {
    pub(super) capacity: String,
    pub(super) status: String,
    pub(super) power: String,
    pub(super) voltage: String,
    /// Degradation health ("87.5%") — present only when the µWh pair is.
    pub(super) health: Option<String>,
    /// The one status-applicable runtime estimate, pre-labeled with its
    /// localized key and formatted through the shared duration formatter;
    /// charging yields time-to-full, discharging time-to-empty, any other
    /// status yields none.
    pub(super) time_estimate: Option<String>,
    pub(super) descriptor: Option<String>,
}

pub(super) fn battery_data(battery: &BatteryInfo) -> BatteryData {
    let capacity = battery
        .current_capacity_pct()
        .map_or_else(missing_value, |value| format!("{value}%"));
    let status = if battery.status.is_empty() {
        missing_value()
    } else {
        battery.status.clone()
    };
    let power = battery
        .current_power_w()
        .map_or_else(missing_value, |value| format!("{value:.1} W"));
    let voltage = battery
        .current_voltage_uv()
        .map_or_else(missing_value, |value| {
            format!("{:.2} V", value as f64 / 1_000_000.0)
        });
    // Health comes from the core full/design rule; each estimate renders
    // only when the native source reported one under its status gate.
    // Unavailable facts stay absent — never a believable "0%" / "0m".
    let health = battery
        .current_health_pct()
        .map(|value| format!("{value:.1}%"));
    let time_estimate = battery
        .current_time_to_full_secs()
        .map(|secs| (t("battery.time_to_full"), secs))
        .or_else(|| {
            battery
                .current_time_to_empty_secs()
                .map(|secs| (t("battery.time_to_empty"), secs))
        })
        .map(|(label, secs)| format!("{} {}", label, duration(secs as u64)));
    let mut descriptor = Vec::new();
    if let Some(cycles) = battery.current_cycle_count() {
        descriptor.push(format!("{} {}", t("battery.cycles"), cycles));
    }
    if !battery.technology.is_empty() {
        descriptor.push(format!(
            "{} {}",
            t("battery.technology"),
            battery.technology
        ));
    }
    if !battery.manufacturer.is_empty() {
        descriptor.push(format!(
            "{} {}",
            t("battery.manufacturer"),
            battery.manufacturer
        ));
    }
    BatteryData {
        capacity,
        status,
        power,
        voltage,
        health,
        time_estimate,
        descriptor: (!descriptor.is_empty()).then(|| format!("  {}", descriptor.join(" · "))),
    }
}

pub(super) struct DiskData {
    pub(super) read: String,
    pub(super) write: String,
    pub(super) active: String,
    pub(super) response_iops: Option<(String, String)>,
    pub(super) capacity_free: Option<(String, String)>,
}

pub(super) fn disk_data(disk: &DiskMetrics, use_bytes: bool, use_base2: bool) -> DiskData {
    let response = disk.current_response_time_ms();
    let iops = disk.current_iops();
    let capacity = disk.current_capacity_bytes();
    let free = disk.current_available_bytes();
    DiskData {
        read: quantity_text_optional(disk.current_read_bytes_per_sec(), use_bytes, use_base2),
        write: quantity_text_optional(disk.current_write_bytes_per_sec(), use_bytes, use_base2),
        active: observed_percentage(disk.current_active_time_pct()),
        response_iops: (response.is_some() || iops.is_some()).then(|| {
            (
                response.map_or_else(missing_value, |ms| format!("{ms:.2} ms")),
                iops.map_or_else(missing_value, |value| value.to_string()),
            )
        }),
        capacity_free: (capacity.is_some() || free.is_some())
            .then(|| (optional_bytes(capacity), optional_bytes(free))),
    }
}

pub(super) struct PartitionData {
    pub(super) capacity: String,
    pub(super) free: String,
}

pub(super) fn partition_data(partition: &DiskPartition) -> PartitionData {
    PartitionData {
        capacity: optional_bytes(partition.current_capacity_bytes()),
        free: optional_bytes(partition.current_free_bytes()),
    }
}

pub(super) fn directory_entry_size(
    entry: &DirectoryUsageEntry,
    use_bytes: bool,
    use_base2: bool,
) -> String {
    quantity_text_optional(
        entry.size_bytes.current_value().copied(),
        use_bytes,
        use_base2,
    )
}

pub(super) fn directory_total_size(
    snapshot: &DirectoryUsageSnapshot,
    use_bytes: bool,
    use_base2: bool,
) -> String {
    quantity_text_optional(
        snapshot.totals.bytes_counted.current_value().copied(),
        use_bytes,
        use_base2,
    )
}

pub(super) struct VramData {
    pub(super) label: &'static str,
    pub(super) used: u64,
    pub(super) total: u64,
}

pub(super) struct GpuData {
    pub(super) utilization: String,
    pub(super) temperature: String,
    pub(super) clock: String,
    pub(super) max_clock: String,
    pub(super) idle_residency: String,
    pub(super) power: Option<String>,
    pub(super) throttle_reason: Option<String>,
    pub(super) vram: Vec<VramData>,
}

pub(super) fn gpu_data(gpu: &GpuMetrics) -> GpuData {
    let mut vram = Vec::new();
    if let (Some(used), Some(total)) = (
        gpu.current_dedicated_vram_used_bytes(),
        gpu.current_dedicated_vram_total_bytes(),
    ) && total > 0
    {
        vram.push(VramData {
            label: t("gpu.dedicated_vram"),
            used,
            total,
        });
    }
    if let (Some(used), Some(total)) = (
        gpu.current_shared_vram_used_bytes(),
        gpu.current_shared_vram_total_bytes(),
    ) && total > 0
    {
        vram.push(VramData {
            label: t("gpu.shared_vram"),
            used,
            total,
        });
    }
    if let (Some(used), Some(total)) = (
        gpu.current_memory_used_bytes(),
        gpu.current_memory_total_bytes(),
    ) {
        vram.push(VramData {
            label: t("gpu.vram"),
            used,
            total,
        });
    }
    GpuData {
        utilization: observed_percentage(gpu.current_utilization_pct()),
        temperature: observed_temperature(gpu.current_temperature_c()),
        clock: observed_frequency(gpu.current_frequency_mhz()),
        max_clock: observed_frequency(gpu.current_max_frequency_mhz()),
        idle_residency: observed_percentage(gpu.current_idle_residency_pct()),
        power: gpu.current_power_w().map(|watts| format!("{watts:.1} W")),
        throttle_reason: gpu
            .current_throttle_reason_text()
            .filter(|reason| !reason.is_empty()),
        vram,
    }
}

pub(super) struct MemoryCompositionData {
    pub(super) total: u64,
    pub(super) used: Option<u64>,
    pub(super) segments: Vec<MemSegment>,
    pub(super) swap: Option<SwapBreakdown>,
}

pub(super) fn memory_composition_data(memory: &MemoryMetrics) -> Option<MemoryCompositionData> {
    let total = memory.current_total_bytes().filter(|total| *total > 0)?;
    Some(MemoryCompositionData {
        total,
        used: memory.current_used_bytes(),
        segments: memory::memory_segments(memory),
        swap: memory::swap_breakdown(memory),
    })
}

pub(super) struct WirelessData {
    pub(super) ssid: String,
    pub(super) signal: String,
    pub(super) bssid: Option<String>,
    pub(super) details: Vec<String>,
}

pub(super) struct NetworkData {
    pub(super) rx: String,
    pub(super) tx: String,
    pub(super) utilization: String,
    pub(super) link: String,
    pub(super) connection: String,
    pub(super) totals: Option<(String, String)>,
    pub(super) wireless: Option<WirelessData>,
}

#[must_use]
pub(super) fn signal_quality_pct(dbm: i32) -> f32 {
    ((dbm as f32 + 90.0) / 60.0 * 100.0).clamp(0.0, 100.0)
}

pub(super) fn network_data(
    network: &NetworkMetrics,
    use_bytes: bool,
    use_base2: bool,
) -> NetworkData {
    let total_rx = network.current_total_rx_bytes();
    let total_tx = network.current_total_tx_bytes();
    let established = network.current_link_up().unwrap_or_else(|| {
        network
            .ipv4_addr
            .as_deref()
            .is_some_and(|address| !address.is_empty())
            || network
                .ipv6_addr
                .as_deref()
                .is_some_and(|address| !address.is_empty())
    });
    let wireless = (network.adapter_type() == NetworkAdapterType::WiFi).then(|| {
        let mut details = Vec::new();
        if let Some(protocol) = network.current_protocol() {
            details.push(format!("{} {protocol}", t("net.protocol")));
        }
        if let Some(channel) = network.current_channel() {
            details.push(format!("{} {channel}", t("net.channel")));
        }
        if let Some(frequency) = network.current_frequency_mhz() {
            details.push(format!("{} {frequency} MHz", t("net.frequency")));
        }
        if let Some(rate) = network.current_rx_bitrate_mbps() {
            details.push(format!("{} {rate} Mbps", t("net.rx_rate")));
        }
        if let Some(rate) = network.current_tx_bitrate_mbps() {
            details.push(format!("{} {rate} Mbps", t("net.tx_rate")));
        }
        WirelessData {
            ssid: network.current_ssid().unwrap_or(MISSING_VALUE).to_owned(),
            signal: network
                .current_signal_dbm()
                .map_or_else(missing_value, |dbm| {
                    let quality = signal_quality_pct(dbm);
                    format!("{dbm} dBm ({quality:.0}%)")
                }),
            bssid: network.current_bssid().map(str::to_owned),
            details,
        }
    });
    NetworkData {
        rx: quantity_text_optional(network.current_rx_bytes_per_sec(), use_bytes, use_base2),
        tx: quantity_text_optional(network.current_tx_bytes_per_sec(), use_bytes, use_base2),
        utilization: observed_percentage(network.current_utilization_pct()),
        link: network
            .current_link_speed_mbps()
            .map_or_else(missing_value, |mbps| format!("{mbps} Mbps")),
        connection: if established {
            t("common.connected").to_owned()
        } else {
            t("common.disconnected").to_owned()
        },
        totals: (total_rx.is_some() || total_tx.is_some()).then(|| {
            (
                quantity_text_optional(total_rx, use_bytes, use_base2),
                quantity_text_optional(total_tx, use_bytes, use_base2),
            )
        }),
        wireless,
    }
}
