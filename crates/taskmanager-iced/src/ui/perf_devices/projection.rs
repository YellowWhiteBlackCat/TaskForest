//! Pure typed-observation projection for Performance device view models.

use taskmanager_application::{
    BatteryInfo, DiskMetrics, DiskPartition, GpuEngine, GpuMetrics, NetworkMetrics,
};

/// Responsive GPU chart composition. This is derived from the frame budget's
/// typed chart inventory, never selected by the user: the Full inventory adds
/// every available engine history below the fixed aggregate graph; the
/// AggregateOnly inventory keeps the aggregate (GPUI `GpuChartLayout::
/// for_chart_inventory` parity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GpuChartLayout {
    AggregateWithEngines,
    AggregateOnly,
}

impl GpuChartLayout {
    #[must_use]
    pub(super) const fn for_inventory(
        inventory: super::super::responsive::PerformanceChartInventory,
    ) -> Self {
        match inventory {
            super::super::responsive::PerformanceChartInventory::AggregateOnly => {
                Self::AggregateOnly
            }
            super::super::responsive::PerformanceChartInventory::Full => Self::AggregateWithEngines,
        }
    }

    /// Every real engine is visible in the standard fixed layout. Compact
    /// space has one aggregate chart and therefore projects no engine charts.
    pub(super) fn engine_charts(self, gpu: &GpuMetrics) -> impl Iterator<Item = &GpuEngine> {
        gpu.engines.iter().filter(move |engine| {
            self == Self::AggregateWithEngines
                && !engine.name.trim().is_empty()
                && engine.usage_pct.is_finite()
        })
    }

    #[must_use]
    pub(super) const fn shows_secondary_regions(self) -> bool {
        matches!(self, Self::AggregateWithEngines)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum GpuHeadlineValue {
    UtilizationPercent(f32),
    TemperatureC(f32),
    FrequencyMhz(u64),
    PowerW(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GpuHeadlineKind {
    Utilization,
    Temperature,
    Frequency,
    Power,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GpuHeadlineMetric {
    pub kind: GpuHeadlineKind,
    pub value: Option<GpuHeadlineValue>,
}

/// Fixed compact-GPU readout order. These are current typed observations,
/// independent of the history vocabulary and any retired graph selector.
pub(super) fn gpu_headline_metrics(gpu: &GpuMetrics) -> [GpuHeadlineMetric; 4] {
    let observed = GpuObservation::from(gpu);
    [
        GpuHeadlineMetric {
            kind: GpuHeadlineKind::Utilization,
            value: observed
                .utilization_pct
                .map(GpuHeadlineValue::UtilizationPercent),
        },
        GpuHeadlineMetric {
            kind: GpuHeadlineKind::Temperature,
            value: observed.temperature_c.map(GpuHeadlineValue::TemperatureC),
        },
        GpuHeadlineMetric {
            kind: GpuHeadlineKind::Frequency,
            value: observed.frequency_mhz.map(GpuHeadlineValue::FrequencyMhz),
        },
        GpuHeadlineMetric {
            kind: GpuHeadlineKind::Power,
            value: observed.power_w.map(GpuHeadlineValue::PowerW),
        },
    ]
}

#[derive(Clone, Copy, Debug)]
pub(super) struct BatteryObservation {
    pub capacity_pct: Option<u8>,
    pub power_w: Option<f32>,
    pub voltage_uv: Option<u64>,
    pub cycle_count: Option<u32>,
    pub health_pct: Option<f64>,
    pub time_to_full_secs: Option<f64>,
    pub time_to_empty_secs: Option<f64>,
}

impl From<&BatteryInfo> for BatteryObservation {
    fn from(battery: &BatteryInfo) -> Self {
        Self {
            capacity_pct: battery.current_capacity_pct(),
            power_w: battery.current_power_w(),
            voltage_uv: battery.current_voltage_uv(),
            cycle_count: battery.current_cycle_count(),
            health_pct: battery.current_health_pct(),
            time_to_full_secs: battery.current_time_to_full_secs(),
            time_to_empty_secs: battery.current_time_to_empty_secs(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PartitionObservation {
    pub used_bytes: Option<u64>,
    pub capacity_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
}

impl From<&DiskPartition> for PartitionObservation {
    fn from(partition: &DiskPartition) -> Self {
        Self {
            used_bytes: partition.current_used_bytes(),
            capacity_bytes: partition.current_capacity_bytes(),
            free_bytes: partition.current_free_bytes(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct DiskObservation {
    pub read_bytes_per_sec: Option<u64>,
    pub write_bytes_per_sec: Option<u64>,
    pub active_time_pct: Option<f32>,
    pub response_time_ms: Option<f32>,
    pub iops: Option<u64>,
    pub capacity_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub partitions: Vec<PartitionObservation>,
}

impl From<&DiskMetrics> for DiskObservation {
    fn from(disk: &DiskMetrics) -> Self {
        Self {
            read_bytes_per_sec: disk.current_read_bytes_per_sec(),
            write_bytes_per_sec: disk.current_write_bytes_per_sec(),
            active_time_pct: disk.current_active_time_pct(),
            response_time_ms: disk.current_response_time_ms(),
            iops: disk.current_iops(),
            capacity_bytes: disk.current_capacity_bytes(),
            available_bytes: disk.current_available_bytes(),
            partitions: disk
                .partitions
                .iter()
                .map(PartitionObservation::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct GpuObservation {
    pub utilization_pct: Option<f32>,
    pub dedicated_vram_used_bytes: Option<u64>,
    pub dedicated_vram_total_bytes: Option<u64>,
    pub shared_vram_used_bytes: Option<u64>,
    pub shared_vram_total_bytes: Option<u64>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub frequency_mhz: Option<u64>,
    pub max_frequency_mhz: Option<u64>,
    pub idle_residency_pct: Option<f32>,
    pub temperature_c: Option<f32>,
    pub power_w: Option<f32>,
    pub throttle_reason: Option<String>,
}

impl From<&GpuMetrics> for GpuObservation {
    fn from(gpu: &GpuMetrics) -> Self {
        Self {
            utilization_pct: gpu.current_utilization_pct(),
            dedicated_vram_used_bytes: gpu.current_dedicated_vram_used_bytes(),
            dedicated_vram_total_bytes: gpu.current_dedicated_vram_total_bytes(),
            shared_vram_used_bytes: gpu.current_shared_vram_used_bytes(),
            shared_vram_total_bytes: gpu.current_shared_vram_total_bytes(),
            memory_used_bytes: gpu.current_memory_used_bytes(),
            memory_total_bytes: gpu.current_memory_total_bytes(),
            frequency_mhz: gpu.current_frequency_mhz(),
            max_frequency_mhz: gpu.current_max_frequency_mhz(),
            idle_residency_pct: gpu.current_idle_residency_pct(),
            temperature_c: gpu.current_temperature_c(),
            power_w: gpu.current_power_w(),
            throttle_reason: gpu.current_throttle_reason_text(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct NetworkObservation {
    pub link_up: Option<bool>,
    pub rx_bytes_per_sec: Option<u64>,
    pub tx_bytes_per_sec: Option<u64>,
    pub link_speed_mbps: Option<u64>,
    pub total_rx_bytes: Option<u64>,
    pub total_tx_bytes: Option<u64>,
    pub utilization_pct: Option<f32>,
    pub ssid: Option<String>,
    pub signal_dbm: Option<i32>,
    pub bssid: Option<String>,
    pub protocol: Option<String>,
    pub channel: Option<u32>,
    pub frequency_mhz: Option<u32>,
    pub rx_bitrate_mbps: Option<u64>,
    pub tx_bitrate_mbps: Option<u64>,
}

impl From<&NetworkMetrics> for NetworkObservation {
    fn from(network: &NetworkMetrics) -> Self {
        Self {
            link_up: network.current_link_up(),
            rx_bytes_per_sec: network.current_rx_bytes_per_sec(),
            tx_bytes_per_sec: network.current_tx_bytes_per_sec(),
            link_speed_mbps: network.current_link_speed_mbps(),
            total_rx_bytes: network.current_total_rx_bytes(),
            total_tx_bytes: network.current_total_tx_bytes(),
            utilization_pct: network.current_utilization_pct(),
            ssid: network.current_ssid().map(str::to_owned),
            signal_dbm: network.current_signal_dbm(),
            bssid: network.current_bssid().map(str::to_owned),
            protocol: network.current_protocol().map(str::to_owned),
            channel: network.current_channel(),
            frequency_mhz: network.current_frequency_mhz(),
            rx_bitrate_mbps: network.current_rx_bitrate_mbps(),
            tx_bitrate_mbps: network.current_tx_bitrate_mbps(),
        }
    }
}
