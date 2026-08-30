//! Pure observation folds for TUI Performance device surfaces.
//!
//! Ratatui renderers consume these grouped, display-ready projections. Typed
//! availability is resolved once here, so paint code never reads a live
//! `current_*` scalar or recreates dash/zero semantics per widget.

use taskmanager_application::i18n::t;
use taskmanager_core::core::directory_usage::{DirectoryUsageEntry, DirectoryUsageSnapshot};
use taskmanager_core::core::metrics::{
    CpuMetrics, DiskMetrics, DiskPartition, GpuMetrics, MemoryMetrics, NetworkAdapterType,
    NetworkMetrics,
};
use taskmanager_core::core::power::BatteryInfo;
use taskmanager_core::core::units::format_quantity_with;
use taskmanager_shell::memory::{self, MemSegment, SwapBreakdown};
use taskmanager_shell::presentation::{
    MISSING_VALUE, missing_value, optional_bytes, temperature_c,
};

use super::units::{
    memory_text_pref, observed_frequency, observed_percentage, observed_temperature,
    quantity_text_optional,
};
use taskmanager_shell::presentation::duration;

/// Bytes in one mebibyte — the signed memory usage rate is published in
/// MiB/s and rendered through the shared byte ladder.
const MIB_BYTES: u64 = 1024 * 1024;

/// Below this absolute rate (MiB/s) the usage-rate row reports the shared
/// dash instead of a sub-tick value, mirroring the gpui row gate
/// (`memory_stats.rs`): a rate the kernel cannot distinguish from zero is
/// honest absence, not a fabricated `+0 MiB/s`.
const USAGE_RATE_NOISE_FLOOR_MIB_PER_SEC: f32 = 0.05;

/// One core grid cell's three-segment readout, folded from the core's
/// current typed observations (utilization · frequency · temperature). A
/// segment whose observation is unobserved or non-finite renders the shared
/// dash, never a fabricated zero; the renderer only joins this with the trend.
pub(super) fn core_cell_readout(cpu: Option<&CpuMetrics>, core_index: usize) -> String {
    [
        cpu.and_then(|cpu| cpu.current_core_usage_pct(core_index))
            .filter(|value| value.is_finite())
            .map_or_else(missing_value, |value| format!("{value:.0}%")),
        cpu.and_then(|cpu| cpu.current_core_frequency_mhz(core_index))
            .map_or_else(missing_value, |mhz| {
                format!("{:.2} GHz", mhz as f64 / 1000.0)
            }),
        cpu.and_then(|cpu| cpu.current_core_temperature_c(core_index))
            .filter(|value| value.is_finite())
            .map_or_else(missing_value, temperature_c),
    ]
    .join(" · ")
}

/// The number of labelled memory-stats rows this snapshot carries: the six
/// fixed rows stay labelled with an honest dash when their fact is
/// unavailable, while the Buffers row keeps the gpui conditional-row
/// semantics (only when the host reports the counter).
pub(super) fn memory_stats_row_count(memory: &MemoryMetrics) -> u16 {
    6 + u16::from(memory.current_buffers_bytes().is_some())
}

/// The labelled memory-stats row set (`perf_views/memory_stats.rs` parity):
/// every value routes through the same typed `MemoryMetrics` accessors the
/// gpui rows read, so an unavailable observation resolves to the shared dash
/// here and the renderer only styles the pair.
pub(super) fn memory_stats_rows(
    memory: &MemoryMetrics,
    use_bytes: bool,
    use_base2: bool,
) -> Vec<(String, String)> {
    let readout = |value: u64| memory_text_pref(value, use_bytes, use_base2);
    let slots = match (memory.current_slots_used(), memory.current_slots_total()) {
        (Some(used), Some(total)) => format!("{used} / {total}"),
        _ => missing_value(),
    };
    // The gpui committed readout only reports the pair once a real limit
    // exists; a committed counter without a limit stays a dash.
    let committed = match (
        memory.current_committed_bytes(),
        memory.current_commit_limit_bytes(),
    ) {
        (Some(committed), Some(limit)) if limit > 0 => {
            format!("{} / {}", readout(committed), readout(limit))
        }
        _ => missing_value(),
    };
    let usage_rate = memory
        .current_used_rate_mib_per_sec()
        .filter(|rate| rate.abs() >= USAGE_RATE_NOISE_FLOOR_MIB_PER_SEC)
        .map_or_else(missing_value, |rate| {
            signed_memory_rate_readout(rate, use_bytes, use_base2)
        });

    let mut rows: Vec<(String, String)> = vec![
        (
            t("mem.available").to_string(),
            memory
                .projected_available_bytes()
                .map_or_else(missing_value, readout),
        ),
        (
            t("mem.hardware_reserved").to_string(),
            memory
                .current_hardware_reserved_bytes()
                .map_or_else(missing_value, readout),
        ),
        (
            t("common.speed").to_string(),
            memory
                .current_speed_mhz()
                .map_or_else(missing_value, |speed| format!("{speed} MT/s")),
        ),
        (t("mem.slots").to_string(), slots),
        (t("mem.committed").to_string(), committed),
        (t("mem.usage_rate").to_string(), usage_rate),
    ];
    if let Some(buffers) = memory.current_buffers_bytes() {
        rows.push((t("mem.buffers").to_string(), readout(buffers)));
    }
    rows
}

/// The signed memory usage-rate readout: an explicit `+`/`-` sign (ASCII, so
/// every terminal profile keeps one cell per glyph) over the shared byte
/// ladder formatted as a per-second quantity.
fn signed_memory_rate_readout(rate_mib_per_sec: f32, use_bytes: bool, use_base2: bool) -> String {
    let sign = if rate_mib_per_sec < 0.0 { '-' } else { '+' };
    let magnitude_bytes = (f64::from(rate_mib_per_sec.abs()) * MIB_BYTES as f64).round() as u64;
    format!(
        "{sign}{}",
        format_quantity_with(magnitude_bytes, use_bytes, use_base2, true)
    )
}

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
        connection: network
            .current_link_up()
            .map_or_else(missing_value, |established| {
                if established {
                    t("common.connected").to_owned()
                } else {
                    t("common.disconnected").to_owned()
                }
            }),
        totals: (total_rx.is_some() || total_tx.is_some()).then(|| {
            (
                quantity_text_optional(total_rx, use_bytes, use_base2),
                quantity_text_optional(total_tx, use_bytes, use_base2),
            )
        }),
        wireless,
    }
}
