//! Memory system-telemetry domain collector.
//!
//! Owns `LinuxMemoryTelemetryCollector`, which keeps a private sysinfo refresh
//! and used-memory rate baseline separate from other domains.

use std::time::Instant;

use sysinfo::System;
use taskmanager_core::{MemoryMetrics, MemoryTelemetryObservation};

use super::{LinuxSystemDomainCollector, SourceQuality, source_quality};
use crate::engine::collector::compute::collect_memory;

/// Memory-only Linux collector with a private sysinfo refresh and rate state.
pub(crate) struct LinuxMemoryTelemetryCollector {
    system: System,
    previous_used: Option<(u64, Instant)>,
    last_value: Option<(MemoryMetrics, u64)>,
}

impl LinuxMemoryTelemetryCollector {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            system: System::new(),
            previous_used: None,
            last_value: None,
        }
    }

    pub(crate) fn observe(&mut self, now: Instant, now_ms: u64) -> MemoryTelemetryObservation {
        <Self as LinuxSystemDomainCollector>::observe(self, now, now_ms)
    }
}

impl Default for LinuxMemoryTelemetryCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSystemDomainCollector for LinuxMemoryTelemetryCollector {
    type Observation = MemoryTelemetryObservation;

    fn observe(&mut self, now: Instant, now_ms: u64) -> Self::Observation {
        self.system.refresh_memory();
        let mut snapshot = collect_memory(&self.system, &mut self.previous_used, now, now_ms);
        if let Some((previous, _)) = &self.last_value {
            snapshot.value.retain_previous_observations(previous);
        }
        let quality = source_quality(&snapshot.sources);
        match quality {
            SourceQuality::Current => {
                self.last_value = Some((snapshot.value.clone(), now_ms));
                MemoryTelemetryObservation::current(snapshot.value, now_ms, snapshot.sources)
            }
            SourceQuality::Partial(failure) => {
                self.last_value = Some((snapshot.value.clone(), now_ms));
                MemoryTelemetryObservation::partial(
                    snapshot.value,
                    now_ms,
                    failure,
                    snapshot.sources,
                )
            }
            SourceQuality::Unavailable(failure) => self.last_value.as_ref().map_or_else(
                || MemoryTelemetryObservation::unavailable(failure, snapshot.sources.clone()),
                |(last_value, last_success_ms)| {
                    MemoryTelemetryObservation::stale(
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
