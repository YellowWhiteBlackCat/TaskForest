//! CPU system-telemetry domain collector.
//!
//! Owns `LinuxCpuTelemetryCollector`, which keeps a private sysinfo refresh,
//! RAPL energy baseline, and detected cache facts separate from other domains.

use std::time::Instant;

use sysinfo::System;
use taskmanager_core::{CpuMetrics, CpuTelemetryObservation};

use super::{LinuxSystemDomainCollector, SourceQuality, source_quality};
use crate::engine::collector::compute::collect_cpu;
use crate::engine::hardware::{detect_cpu_cache, read_sysfs_u64};

/// CPU-only Linux collector with a private sysinfo refresh and RAPL baseline.
pub(crate) struct LinuxCpuTelemetryCollector {
    system: System,
    previous_rapl: Option<(u64, Instant)>,
    cache_kb: (Option<u64>, Option<u64>, Option<u64>, Option<u64>),
    rapl_max_energy_uj: u64,
    static_facts_initialized: bool,
    last_value: Option<(CpuMetrics, u64)>,
}

impl LinuxCpuTelemetryCollector {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            system: System::new(),
            previous_rapl: None,
            cache_kb: (None, None, None, None),
            rapl_max_energy_uj: 1u64 << 32,
            static_facts_initialized: false,
            last_value: None,
        }
    }

    pub(crate) fn observe(&mut self, now: Instant, now_ms: u64) -> CpuTelemetryObservation {
        <Self as LinuxSystemDomainCollector>::observe(self, now, now_ms)
    }
}

impl Default for LinuxCpuTelemetryCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSystemDomainCollector for LinuxCpuTelemetryCollector {
    type Observation = CpuTelemetryObservation;

    fn observe(&mut self, now: Instant, now_ms: u64) -> Self::Observation {
        if !self.static_facts_initialized {
            self.cache_kb = detect_cpu_cache();
            self.rapl_max_energy_uj =
                read_sysfs_u64("/sys/class/powercap/intel-rapl:0/max_energy_range_uj")
                    .unwrap_or(1u64 << 32);
            self.static_facts_initialized = true;
        }
        self.system.refresh_cpu_all();
        let mut snapshot = collect_cpu(
            &self.system,
            &mut self.previous_rapl,
            self.cache_kb,
            self.rapl_max_energy_uj,
            now,
            now_ms,
        );
        if let Some((previous, _)) = &self.last_value {
            snapshot.value.retain_previous_observations(previous);
        }
        let quality = source_quality(&snapshot.sources);
        match quality {
            SourceQuality::Current => {
                self.last_value = Some((snapshot.value.clone(), now_ms));
                CpuTelemetryObservation::current(snapshot.value, now_ms, snapshot.sources)
            }
            SourceQuality::Partial(failure) => {
                self.last_value = Some((snapshot.value.clone(), now_ms));
                CpuTelemetryObservation::partial(snapshot.value, now_ms, failure, snapshot.sources)
            }
            SourceQuality::Unavailable(failure) => self.last_value.as_ref().map_or_else(
                || CpuTelemetryObservation::unavailable(failure, snapshot.sources.clone()),
                |(last_value, last_success_ms)| {
                    CpuTelemetryObservation::stale(
                        last_value.clone(),
                        *last_success_ms,
                        failure,
                        snapshot.sources.clone(),
                    )
                },
            ),
        }
    }
}
