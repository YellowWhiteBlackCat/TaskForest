//! GPU system-telemetry domain collector.
//!
//! Owns `LinuxGpuTelemetryCollector`, which holds the runtime-selected
//! all-hardware provider registry and GPU device lifecycle state.

use std::time::Instant;

use taskmanager_core::{
    DEFAULT_DEVICE_ABSENCE_RETENTION_MS, DeviceLifecycleRegistry, DeviceRefreshOutcome,
    DeviceStatus, GpuMetrics, GpuTelemetryObservation,
};

use super::{LinuxSystemDomainCollector, SourceQuality, device_quality, lifecycle_snapshot};
use crate::engine::collector::lifecycle::reconcile_devices;
use crate::engine::hardware::GpuProviderRegistry;

/// GPU-only collector owning the runtime-selected all-hardware provider
/// registry and GPU lifecycle state.
pub(crate) struct LinuxGpuTelemetryCollector {
    registry: GpuProviderRegistry,
    lifecycles: DeviceLifecycleRegistry,
    last_value: Option<(Vec<GpuMetrics>, u64)>,
}

impl LinuxGpuTelemetryCollector {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            registry: GpuProviderRegistry::standard(),
            lifecycles: DeviceLifecycleRegistry::new(DEFAULT_DEVICE_ABSENCE_RETENTION_MS),
            last_value: None,
        }
    }

    pub(crate) fn observe(&mut self, now: Instant, now_ms: u64) -> GpuTelemetryObservation {
        <Self as LinuxSystemDomainCollector>::observe(self, now, now_ms)
    }
}

impl Default for LinuxGpuTelemetryCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSystemDomainCollector for LinuxGpuTelemetryCollector {
    type Observation = GpuTelemetryObservation;

    fn observe(&mut self, now: Instant, now_ms: u64) -> Self::Observation {
        let snapshot = self.registry.collect(now, now_ms);
        let refresh = snapshot
            .authoritative_refresh
            .unwrap_or(DeviceRefreshOutcome::Unavailable(DeviceStatus::Stale));
        let mut metrics = snapshot.metrics;
        let lifecycle_delta =
            reconcile_devices(&mut self.lifecycles, &mut metrics, refresh, now_ms);
        self.registry
            .prune_generations(&lifecycle_delta.newly_absent);
        self.registry.prune_generations(&lifecycle_delta.expired);

        let lifecycles = lifecycle_snapshot(&self.lifecycles);
        let quality = device_quality(
            match refresh {
                DeviceRefreshOutcome::Complete => taskmanager_core::SourceOutcome::Available,
                DeviceRefreshOutcome::Unavailable(status) => {
                    taskmanager_core::SourceOutcome::Unavailable(
                        status
                            .failure()
                            .unwrap_or(taskmanager_core::FailureKind::ProviderFault),
                    )
                }
            },
            !metrics.is_empty(),
            &snapshot.sources,
        );
        match quality {
            SourceQuality::Current => {
                self.last_value = Some((metrics.clone(), now_ms));
                GpuTelemetryObservation::current(
                    metrics,
                    now_ms,
                    snapshot.sources,
                    snapshot.provider_states,
                    lifecycles,
                )
            }
            SourceQuality::Partial(failure) => {
                self.last_value = Some((metrics.clone(), now_ms));
                GpuTelemetryObservation::partial(
                    metrics,
                    now_ms,
                    failure,
                    snapshot.sources,
                    snapshot.provider_states,
                    lifecycles,
                )
            }
            SourceQuality::Unavailable(failure) => self.last_value.as_ref().map_or_else(
                || {
                    GpuTelemetryObservation::unavailable(
                        failure,
                        snapshot.sources.clone(),
                        snapshot.provider_states.clone(),
                        lifecycles.clone(),
                    )
                },
                |(last_value, last_success_ms)| {
                    GpuTelemetryObservation::stale(
                        last_value.clone(),
                        *last_success_ms,
                        failure,
                        snapshot.sources.clone(),
                        snapshot.provider_states.clone(),
                        lifecycles.clone(),
                    )
                },
            ),
        }
    }
}
