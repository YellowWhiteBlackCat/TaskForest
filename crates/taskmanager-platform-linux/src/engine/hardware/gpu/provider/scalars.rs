//! Provider-neutral scalar assembly for the runtime GPU registry.

use std::collections::BTreeMap;

use taskmanager_core::{
    DeviceId, FailureKind, GpuMetricField, GpuMetrics, GpuScalarObservations, GpuThrottleReason,
    ProviderId, ScalarObservation,
};

use super::receipts::GpuFieldFailures;

#[derive(Default)]
pub(super) struct GpuScalarTracker {
    devices: BTreeMap<String, TrackedGpuScalars>,
}

#[derive(Default)]
struct TrackedGpuScalars {
    observations: GpuScalarObservations,
    throttle_observation: ScalarObservation<Vec<GpuThrottleReason>>,
    providers: BTreeMap<GpuMetricField, ProviderId>,
}

#[derive(Clone, Copy)]
struct FieldReceipt {
    partial_failure: Option<FailureKind>,
    missing_failure: FailureKind,
}

impl GpuScalarTracker {
    pub(super) fn observe(
        &mut self,
        metrics: &mut [GpuMetrics],
        provider_failures: &BTreeMap<ProviderId, FailureKind>,
        field_failures: &GpuFieldFailures,
        observed_at_ms: u64,
    ) {
        for metric in metrics {
            let tracked = self.devices.entry(metric.device_id.clone()).or_default();
            let failures = field_failures.get(&metric.device_id);
            let receipt = |field| {
                let partial_failure = failures.and_then(|failures| failures.get(&field)).copied();
                FieldReceipt {
                    partial_failure,
                    missing_failure: partial_failure.unwrap_or_else(|| {
                        field_failure(&tracked.providers, provider_failures, field)
                    }),
                }
            };
            let current = GpuScalarObservations {
                utilization_pct: observe_percentage(
                    metric.current_utilization_pct(),
                    observed_at_ms,
                    receipt(GpuMetricField::Utilization),
                ),
                temperature_c: observe_temperature(
                    metric.current_temperature_c(),
                    observed_at_ms,
                    receipt(GpuMetricField::Temperature),
                ),
                memory_used_bytes: observe_optional(
                    metric.current_memory_used_bytes(),
                    observed_at_ms,
                    receipt(GpuMetricField::Memory),
                ),
                memory_total_bytes: observe_optional(
                    metric.current_memory_total_bytes(),
                    observed_at_ms,
                    receipt(GpuMetricField::Memory),
                ),
                dedicated_vram_used_bytes: observe_optional(
                    metric.current_dedicated_vram_used_bytes(),
                    observed_at_ms,
                    receipt(GpuMetricField::DedicatedVram),
                ),
                dedicated_vram_total_bytes: observe_optional(
                    metric.current_dedicated_vram_total_bytes(),
                    observed_at_ms,
                    receipt(GpuMetricField::DedicatedVram),
                ),
                shared_vram_used_bytes: observe_optional(
                    metric.current_shared_vram_used_bytes(),
                    observed_at_ms,
                    receipt(GpuMetricField::SharedVram),
                ),
                shared_vram_total_bytes: observe_optional(
                    metric.current_shared_vram_total_bytes(),
                    observed_at_ms,
                    receipt(GpuMetricField::SharedVram),
                ),
                frequency_mhz: observe_positive(
                    metric.current_frequency_mhz(),
                    observed_at_ms,
                    receipt(GpuMetricField::Frequency),
                ),
                max_frequency_mhz: observe_positive(
                    metric.current_max_frequency_mhz(),
                    observed_at_ms,
                    receipt(GpuMetricField::Frequency),
                ),
                fan_speed_rpm: observe_optional(
                    metric.current_fan_speed_rpm(),
                    observed_at_ms,
                    receipt(GpuMetricField::Fan),
                ),
                fan_speed_pct: observe_percentage(
                    metric.current_fan_speed_pct(),
                    observed_at_ms,
                    receipt(GpuMetricField::Fan),
                ),
                power_w: observe_nonnegative(
                    metric.current_power_w(),
                    observed_at_ms,
                    receipt(GpuMetricField::Power),
                ),
                idle_residency_pct: observe_percentage(
                    metric.current_idle_residency_pct(),
                    observed_at_ms,
                    receipt(GpuMetricField::IdleResidency),
                ),
            }
            .retain_previous(tracked.observations);
            let throttle_receipt = receipt(GpuMetricField::Throttle);
            let throttle_observation = metric.current_throttle_reasons().map_or_else(
                || ScalarObservation::unavailable(throttle_receipt.missing_failure),
                |reasons| {
                    observe_value(
                        reasons.to_vec(),
                        observed_at_ms,
                        throttle_receipt.partial_failure,
                    )
                },
            );
            let throttle_observation =
                throttle_observation.retain_previous(tracked.throttle_observation.clone());

            for provenance in &metric.provenance {
                tracked
                    .providers
                    .insert(provenance.field, provenance.provider.clone());
            }
            tracked.observations = current;
            tracked.throttle_observation = throttle_observation.clone();
            metric.apply_scalar_observations(current);
            metric.apply_throttle_observation(throttle_observation);
        }
    }

    pub(super) fn prune(&mut self, device_ids: &[DeviceId]) {
        for device_id in device_ids {
            self.devices.remove(device_id.as_str());
        }
    }
}

fn field_failure(
    providers: &BTreeMap<GpuMetricField, ProviderId>,
    failures: &BTreeMap<ProviderId, FailureKind>,
    field: GpuMetricField,
) -> FailureKind {
    providers
        .get(&field)
        .and_then(|provider| failures.get(provider))
        .copied()
        .unwrap_or(FailureKind::Unsupported)
}

fn observe_optional<T: Copy>(
    value: Option<T>,
    observed_at_ms: u64,
    receipt: FieldReceipt,
) -> ScalarObservation<T> {
    value.map_or_else(
        || ScalarObservation::unavailable(receipt.missing_failure),
        |value| observe_value(value, observed_at_ms, receipt.partial_failure),
    )
}

fn observe_positive(
    value: Option<u64>,
    observed_at_ms: u64,
    receipt: FieldReceipt,
) -> ScalarObservation<u64> {
    match value {
        Some(value) if value > 0 => observe_value(value, observed_at_ms, receipt.partial_failure),
        Some(_) => ScalarObservation::unavailable(FailureKind::ProviderFault),
        None => ScalarObservation::unavailable(receipt.missing_failure),
    }
}

fn observe_percentage(
    value: Option<f32>,
    observed_at_ms: u64,
    receipt: FieldReceipt,
) -> ScalarObservation<f32> {
    match value {
        Some(value) if value.is_finite() && (0.0..=100.0).contains(&value) => {
            observe_value(value, observed_at_ms, receipt.partial_failure)
        }
        Some(_) => ScalarObservation::unavailable(FailureKind::ProviderFault),
        None => ScalarObservation::unavailable(receipt.missing_failure),
    }
}

fn observe_temperature(
    value: Option<f32>,
    observed_at_ms: u64,
    receipt: FieldReceipt,
) -> ScalarObservation<f32> {
    match value {
        Some(value) if value.is_finite() && (-273.15..=1_000.0).contains(&value) => {
            observe_value(value, observed_at_ms, receipt.partial_failure)
        }
        Some(_) => ScalarObservation::unavailable(FailureKind::ProviderFault),
        None => ScalarObservation::unavailable(receipt.missing_failure),
    }
}

fn observe_nonnegative(
    value: Option<f32>,
    observed_at_ms: u64,
    receipt: FieldReceipt,
) -> ScalarObservation<f32> {
    match value {
        Some(value) if value.is_finite() && value >= 0.0 => {
            observe_value(value, observed_at_ms, receipt.partial_failure)
        }
        Some(_) => ScalarObservation::unavailable(FailureKind::ProviderFault),
        None => ScalarObservation::unavailable(receipt.missing_failure),
    }
}

fn observe_value<T>(
    value: T,
    observed_at_ms: u64,
    partial_failure: Option<FailureKind>,
) -> ScalarObservation<T> {
    match partial_failure {
        Some(failure) => ScalarObservation::partial(value, observed_at_ms, failure),
        None => ScalarObservation::available(value, observed_at_ms),
    }
}

#[cfg(test)]
#[path = "../../../../../tests/headless/linux_engine_hardware_gpu_provider_scalars_tests.rs"]
mod tests;
