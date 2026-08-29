//! Pure CPU and memory observation view models for the Performance overview.

use taskmanager_core::core::metrics::{CpuMetrics, MemoryMetrics};

/// Responsive CPU composition derived from the frame budget's typed chart
/// inventory (GPUI `CpuChartLayout::for_inventory` parity). This is layout
/// state, not user-selectable mode: the Full inventory adds the bounded
/// context surfaces below the aggregate headline; AggregateOnly keeps the
/// aggregate legible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CpuChartLayout {
    AggregateWithPerCore,
    AggregateOnly,
}

impl CpuChartLayout {
    #[must_use]
    pub(super) const fn for_inventory(
        inventory: super::super::responsive::PerformanceChartInventory,
    ) -> Self {
        match inventory {
            super::super::responsive::PerformanceChartInventory::AggregateOnly => {
                Self::AggregateOnly
            }
            super::super::responsive::PerformanceChartInventory::Full => Self::AggregateWithPerCore,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CpuObservation {
    pub usage_pct: Option<f32>,
    pub frequency_mhz: Option<u64>,
    pub temperature_c: Option<f32>,
    pub power_w: Option<f32>,
}

impl From<&CpuMetrics> for CpuObservation {
    fn from(cpu: &CpuMetrics) -> Self {
        Self {
            usage_pct: cpu.current_global_usage_pct(),
            frequency_mhz: cpu.current_frequency_mhz(),
            temperature_c: cpu.current_temperature_c(),
            power_w: cpu.current_power_w().filter(|value| *value > 0.0),
        }
    }
}

/// One typed current CPU reading. Histories remain a separate concern: the
/// Performance page renders one fixed utilization graph and projects the other
/// CPU facts as simultaneous headline readouts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum CpuHeadlineValue {
    UsagePercent(f32),
    TemperatureC(f32),
    FrequencyMhz(u64),
    PowerW(f32),
}

/// One CPU headline item in the canonical product order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CpuHeadlineKind {
    Utilization,
    Frequency,
    Temperature,
    Power,
}

/// One CPU headline item in the Iced presentation order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CpuHeadlineMetric {
    pub kind: CpuHeadlineKind,
    pub value: Option<CpuHeadlineValue>,
}

/// Project the current CPU observations in the fixed headline order. This
/// order is a UI decision and deliberately does not come from the telemetry
/// history vocabulary. Missing provider data stays explicit so the renderer
/// shows a dash instead of a fabricated zero.
pub(super) fn cpu_headline_metrics(observation: Option<CpuObservation>) -> [CpuHeadlineMetric; 4] {
    let observed = observation.unwrap_or(CpuObservation {
        usage_pct: None,
        frequency_mhz: None,
        temperature_c: None,
        power_w: None,
    });
    [
        CpuHeadlineMetric {
            kind: CpuHeadlineKind::Utilization,
            value: observed.usage_pct.map(CpuHeadlineValue::UsagePercent),
        },
        CpuHeadlineMetric {
            kind: CpuHeadlineKind::Frequency,
            value: observed.frequency_mhz.map(CpuHeadlineValue::FrequencyMhz),
        },
        CpuHeadlineMetric {
            kind: CpuHeadlineKind::Temperature,
            value: observed.temperature_c.map(CpuHeadlineValue::TemperatureC),
        },
        CpuHeadlineMetric {
            kind: CpuHeadlineKind::Power,
            value: observed.power_w.map(CpuHeadlineValue::PowerW),
        },
    ]
}

#[derive(Clone, Debug)]
pub(super) struct MemoryObservation {
    pub total_bytes: Option<u64>,
    pub used_bytes: Option<u64>,
    /// Kernel availability with the ZFS ARC layered on (the core
    /// projection — the row set renders this, not the raw kernel fact).
    pub projected_available_bytes: Option<u64>,
    pub swap_total_bytes: Option<u64>,
    pub swap_used_bytes: Option<u64>,
    pub hardware_reserved_bytes: Option<u64>,
    pub cached_bytes: Option<u64>,
    pub buffers_bytes: Option<u64>,
    /// ZFS adaptive replacement cache, when the host reports one.
    pub zfs_arc_bytes: Option<u64>,
    pub speed_mhz: Option<u32>,
    pub slots_used: Option<usize>,
    pub slots_total: Option<usize>,
    pub committed_bytes: Option<u64>,
    pub commit_limit_bytes: Option<u64>,
    pub compressed_memory_used_bytes: Option<u64>,
    pub compressed_swap_used_bytes: Option<u64>,
    pub compressed_swap_capacity_bytes: Option<u64>,
    pub compressed_swap_cache_enabled: Option<bool>,
    pub compressed_swap_original_bytes: Option<u64>,
    pub compressed_swap_compressed_bytes: Option<u64>,
    pub compressed_swap_memory_used_bytes: Option<u64>,
    pub compressed_swap_compression_ratio: Option<f32>,
    pub used_rate_mib_per_sec: Option<f32>,
}

impl From<&MemoryMetrics> for MemoryObservation {
    fn from(memory: &MemoryMetrics) -> Self {
        Self {
            total_bytes: memory.current_total_bytes(),
            used_bytes: memory.current_used_bytes(),
            projected_available_bytes: memory.projected_available_bytes(),
            swap_total_bytes: memory.current_swap_total_bytes(),
            swap_used_bytes: memory.current_swap_used_bytes(),
            hardware_reserved_bytes: memory.current_hardware_reserved_bytes(),
            cached_bytes: memory.current_cached_bytes(),
            buffers_bytes: memory.current_buffers_bytes(),
            zfs_arc_bytes: memory.current_zfs_arc_bytes(),
            speed_mhz: memory.current_speed_mhz(),
            slots_used: memory.current_slots_used(),
            slots_total: memory.current_slots_total(),
            committed_bytes: memory.current_committed_bytes(),
            commit_limit_bytes: memory.current_commit_limit_bytes(),
            compressed_memory_used_bytes: memory.current_compressed_memory_used_bytes(),
            compressed_swap_used_bytes: memory.current_compressed_swap_used_bytes(),
            compressed_swap_capacity_bytes: memory.current_compressed_swap_capacity_bytes(),
            compressed_swap_cache_enabled: memory.current_compressed_swap_cache_enabled(),
            compressed_swap_original_bytes: memory.current_compressed_swap_original_bytes(),
            compressed_swap_compressed_bytes: memory.current_compressed_swap_compressed_bytes(),
            compressed_swap_memory_used_bytes: memory.current_compressed_swap_memory_used_bytes(),
            compressed_swap_compression_ratio: memory.current_compressed_swap_ratio(),
            used_rate_mib_per_sec: memory.current_used_rate_mib_per_sec(),
        }
    }
}
