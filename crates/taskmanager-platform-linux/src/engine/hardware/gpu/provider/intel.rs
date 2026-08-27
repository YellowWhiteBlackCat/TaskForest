//! Intel sysfs GPU telemetry provider (`linux.gpu.intel-sysfs`).
//!
//! Enriches the DRM inventory with GT frequency, RC6 idle-residency deltas and
//! per-engine busy breakdowns shared by the i915 and xe drivers.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use taskmanager_core::{
    DeviceId, DeviceStatus, FailureKind, GpuMetricField, GpuScalarObservations, ProviderId,
    ScalarObservation,
};

use super::super::{
    GpuProviderFieldFailure, GpuProviderSample, build_drm_identity_metrics, preferred_gpu_failure,
    read_intel_gt_engines, read_intel_gt_frequency, read_intel_gt_max_frequency,
    read_intel_gt_rc6_residency, scan_drm_cards,
};
use super::drm::verify_directory;
use super::{GpuProviderFailure, GpuTelemetryProvider};

// The per-engine PMU trackers consume the Linux-only perf-ioctl boundary crate;
// the sysfs identity/RC6 paths above stay compiled on every target.
#[cfg(target_os = "linux")]
mod engines;
mod escalation;
mod rc6;
#[cfg(target_os = "linux")]
mod xe_pmu;
#[cfg(target_os = "linux")]
use engines::{IntelEngineFallback, IntelEngineTracker};
use rc6::IntelRc6Tracker;

pub(super) const INTEL_SYSFS_PROVIDER_ID: ProviderId =
    ProviderId::borrowed("linux.gpu.intel-sysfs");

/// Runtime Intel enrichment shared by i915 and xe; DRM remains authoritative.
pub(super) struct IntelSysfsGpuProvider {
    root: PathBuf,
    rc6: IntelRc6Tracker,
    #[cfg(target_os = "linux")]
    engines: IntelEngineTracker,
    #[cfg(target_os = "linux")]
    pmu: IntelEngineFallback,
}

impl IntelSysfsGpuProvider {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            root,
            rc6: IntelRc6Tracker::default(),
            #[cfg(target_os = "linux")]
            engines: IntelEngineTracker::default(),
            #[cfg(target_os = "linux")]
            pmu: IntelEngineFallback::default(),
        }
    }
}

impl GpuTelemetryProvider for IntelSysfsGpuProvider {
    fn id(&self) -> ProviderId {
        INTEL_SYSFS_PROVIDER_ID
    }

    fn priority(&self) -> u16 {
        50
    }

    fn collect(&mut self, now: Instant) -> Result<Vec<GpuProviderSample>, GpuProviderFailure> {
        verify_directory(&self.root)?;
        let mut saw_intel = false;
        let mut samples = Vec::new();

        for (card_name, device_path) in scan_drm_cards(&self.root) {
            let mut metrics = build_drm_identity_metrics(&card_name, &device_path);
            let is_intel = metrics.brand.starts_with("Intel")
                || matches!(metrics.driver.as_deref(), Some("i915" | "xe"));
            if !is_intel {
                continue;
            }
            saw_intel = true;
            let mut fields = Vec::new();
            let mut failures = BTreeMap::new();
            let mut observations = GpuScalarObservations::default();

            let frequency = read_intel_gt_frequency(&device_path);
            let max_frequency = read_intel_gt_max_frequency(&device_path);
            if let Some(value) = frequency.value {
                observations.frequency_mhz = ScalarObservation::available(value, 0);
            }
            if let Some(value) = max_frequency.value {
                observations.max_frequency_mhz = ScalarObservation::available(value, 0);
            }
            if frequency.value.is_some() || max_frequency.value.is_some() {
                fields.push(GpuMetricField::Frequency);
            }
            record_optional_failure(
                &mut failures,
                GpuMetricField::Frequency,
                preferred_gpu_failure(frequency.failure, max_frequency.failure),
            );

            let idle = self.rc6.observe(
                &metrics.device_id,
                read_intel_gt_rc6_residency(&device_path),
                now,
            );
            if let Some(value) = idle.utilization_pct {
                observations.utilization_pct = ScalarObservation::available(value, 0);
            }
            if let Some(value) = idle.idle_pct {
                observations.idle_residency_pct = ScalarObservation::available(value, 0);
            }
            if idle.utilization_pct.is_some() {
                fields.push(GpuMetricField::Utilization);
                fields.push(GpuMetricField::IdleResidency);
            }
            record_optional_failure(&mut failures, GpuMetricField::Utilization, idle.failure);
            record_optional_failure(&mut failures, GpuMetricField::IdleResidency, idle.failure);
            metrics.apply_scalar_observations(observations);

            // Per-engine busy: sysfs first, then i915 PMU fallback when empty
            // (Linux-only — the PMU boundary crate does not exist elsewhere).
            #[cfg(target_os = "linux")]
            {
                let engine_read = self.pmu.fallback_if_empty(
                    &metrics.device_id,
                    &device_path,
                    read_intel_gt_engines(&device_path),
                    now,
                );
                self.engines
                    .observe(&metrics.device_id, engine_read, now)
                    .apply(&mut metrics, &mut fields, &mut failures);
            }

            samples.push(GpuProviderSample {
                metrics,
                fields,
                field_failures: failures
                    .into_iter()
                    .map(|(field, failure)| GpuProviderFieldFailure { field, failure })
                    .collect(),
            });
        }

        if saw_intel {
            Ok(samples)
        } else {
            Err(GpuProviderFailure::new(DeviceStatus::Unsupported))
        }
    }

    fn prune_generations(&mut self, device_ids: &[DeviceId]) {
        self.rc6.prune(device_ids);
        #[cfg(target_os = "linux")]
        {
            self.engines.prune(device_ids);
            self.pmu.prune(device_ids);
        }
    }
}

fn record_optional_failure(
    failures: &mut BTreeMap<GpuMetricField, FailureKind>,
    field: GpuMetricField,
    failure: Option<FailureKind>,
) {
    if let Some(failure) = failure {
        failures
            .entry(field)
            .and_modify(|current| {
                *current = preferred_gpu_failure(Some(*current), Some(failure)).unwrap_or(failure);
            })
            .or_insert(failure);
    }
}
