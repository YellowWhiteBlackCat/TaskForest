//! Runtime GPU provider registry and deterministic device assembly.
//!
//! Provider traits describe a telemetry capability, never a hardware SKU.
//! Concrete DRM, procfs and optional vendor-library implementations are all
//! registered in the standard artifact and merged by stable PCI/UUID identity.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use taskmanager_core::{
    DeviceId, DeviceRefreshOutcome, DeviceStatus, FailureKind, GpuMetrics, ProviderId,
    ProviderRuntimeState, SourceOutcome, SourceStatus,
};

use super::GpuProviderSample;

mod amd;
mod drm;
#[cfg(not(any(test, feature = "test-support")))]
mod graphics_api;
mod intel;
mod merge;
mod nvidia;
mod receipts;
mod scalars;
#[cfg(test)]
#[path = "../../../../tests/headless/engine/hardware/gpu/provider.rs"]
mod tests;

use amd::AmdSysfsGpuProvider;
use drm::DrmSysfsGpuProvider;
#[cfg(not(any(test, feature = "test-support")))]
use graphics_api::GraphicsApiProvider;
use intel::IntelSysfsGpuProvider;
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", allow(unused_imports))]
pub(super) use merge::merge_provider_samples;
use merge::merge_sample;
use nvidia::NvidiaProcfsGpuProvider;
#[cfg(feature = "nvidia")]
use nvidia::NvmlGpuProvider;
use receipts::{GpuFieldFailures, merge_sample_receipts};
use scalars::GpuScalarTracker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GpuProviderFailure {
    status: DeviceStatus,
}

impl GpuProviderFailure {
    const fn new(status: DeviceStatus) -> Self {
        Self { status }
    }
}

trait GpuTelemetryProvider: Send {
    fn id(&self) -> ProviderId;
    fn priority(&self) -> u16;
    fn is_authoritative_inventory(&self) -> bool {
        false
    }
    fn collect(&mut self, now: Instant) -> Result<Vec<GpuProviderSample>, GpuProviderFailure>;
    fn prune_generations(&mut self, _device_ids: &[DeviceId]) {}
}

struct ProviderEntry {
    provider: Box<dyn GpuTelemetryProvider>,
    last_success_ms: Option<u64>,
}

/// One registry refresh. Device metrics and provider failures are deliberately
/// separate so a failed enhancement source cannot erase a healthy baseline.
#[derive(Debug, Default)]
pub(crate) struct GpuRegistrySnapshot {
    pub(crate) metrics: Vec<GpuMetrics>,
    pub(crate) sources: Vec<SourceStatus>,
    pub(crate) provider_states: Vec<ProviderRuntimeState>,
    pub(crate) authoritative_refresh: Option<DeviceRefreshOutcome>,
}

/// Runtime-selected GPU backends for one native platform artifact.
pub(crate) struct GpuProviderRegistry {
    entries: Vec<ProviderEntry>,
    scalar_tracker: GpuScalarTracker,
}

impl GpuProviderRegistry {
    pub(crate) fn standard() -> Self {
        Self::standard_with_roots(
            PathBuf::from("/sys/class/drm"),
            PathBuf::from("/proc/driver/nvidia"),
        )
    }

    fn standard_with_roots(drm_root: PathBuf, nvidia_root: PathBuf) -> Self {
        let mut registry = Self {
            entries: Vec::new(),
            scalar_tracker: GpuScalarTracker::default(),
        };
        registry.register(DrmSysfsGpuProvider::new(drm_root.clone()));
        #[cfg(not(any(test, feature = "test-support")))]
        registry.register(GraphicsApiProvider::new(drm_root.clone()));
        registry.register(AmdSysfsGpuProvider::new(drm_root.clone()));
        registry.register(IntelSysfsGpuProvider::new(drm_root));
        registry.register(NvidiaProcfsGpuProvider::new(nvidia_root));
        #[cfg(feature = "nvidia")]
        registry.register(NvmlGpuProvider);
        registry
    }

    fn register(&mut self, provider: impl GpuTelemetryProvider + 'static) {
        self.entries.push(ProviderEntry {
            provider: Box::new(provider),
            last_success_ms: None,
        });
        self.entries.sort_by(|left, right| {
            left.provider
                .priority()
                .cmp(&right.provider.priority())
                .then_with(|| left.provider.id().cmp(&right.provider.id()))
        });
    }

    pub(crate) fn collect(&mut self, now: Instant, observed_at_ms: u64) -> GpuRegistrySnapshot {
        let mut devices = BTreeMap::<String, GpuMetrics>::new();
        let mut sources = Vec::with_capacity(self.entries.len());
        let mut provider_states = Vec::with_capacity(self.entries.len());
        let mut authoritative_refresh = None;
        let mut provider_failures = BTreeMap::<ProviderId, FailureKind>::new();
        let mut field_failures = GpuFieldFailures::new();

        for entry in &mut self.entries {
            let provider = entry.provider.id();
            match entry.provider.collect(now) {
                Ok(samples) => {
                    let item_count = samples.len();
                    if entry.provider.is_authoritative_inventory() {
                        authoritative_refresh = Some(DeviceRefreshOutcome::Complete);
                    }
                    entry.last_success_ms = Some(observed_at_ms);
                    for sample in samples {
                        merge_sample_receipts(&devices, &mut field_failures, &sample);
                        merge_sample(&mut devices, provider.clone(), sample, observed_at_ms);
                    }
                    provider_states.push(ProviderRuntimeState {
                        provider: provider.clone(),
                        status: DeviceStatus::Healthy,
                        last_success_ms: entry.last_success_ms,
                    });
                    sources.push(SourceStatus {
                        provider,
                        outcome: if item_count == 0 {
                            SourceOutcome::Empty
                        } else {
                            SourceOutcome::Available
                        },
                        item_count,
                    });
                }
                Err(failure) => {
                    provider_failures.insert(
                        provider.clone(),
                        failure
                            .status
                            .failure()
                            .unwrap_or(FailureKind::ProviderFault),
                    );
                    if entry.provider.is_authoritative_inventory() {
                        authoritative_refresh =
                            Some(DeviceRefreshOutcome::Unavailable(failure.status));
                    }
                    provider_states.push(ProviderRuntimeState {
                        provider: provider.clone(),
                        status: failure.status,
                        last_success_ms: entry.last_success_ms,
                    });
                    sources.push(SourceStatus {
                        provider,
                        outcome: SourceOutcome::Unavailable(
                            failure
                                .status
                                .failure()
                                .unwrap_or(taskmanager_core::FailureKind::ProviderFault),
                        ),
                        item_count: 0,
                    });
                }
            }
        }

        for metric in devices.values_mut() {
            metric.provenance.sort_by_key(|item| item.field);
        }
        provider_states.sort_by(|left, right| left.provider.cmp(&right.provider));
        sources.sort_by(|left, right| left.provider.cmp(&right.provider));
        let mut metrics = devices.into_values().collect::<Vec<_>>();
        self.scalar_tracker.observe(
            &mut metrics,
            &provider_failures,
            &field_failures,
            observed_at_ms,
        );
        GpuRegistrySnapshot {
            metrics,
            sources,
            provider_states,
            authoritative_refresh,
        }
    }

    pub(crate) fn prune_generations(&mut self, device_ids: &[DeviceId]) {
        for entry in &mut self.entries {
            entry.provider.prune_generations(device_ids);
        }
        self.scalar_tracker.prune(device_ids);
    }
}
