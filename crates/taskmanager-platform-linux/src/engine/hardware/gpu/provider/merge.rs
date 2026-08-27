//! Merge typed GPU provider samples into the device-scoped capability view.

use std::collections::BTreeMap;

use taskmanager_core::{GpuMetricField, GpuMetricProvenance, GpuMetrics, ProviderId};

use super::super::{GpuProviderSample, apply_gpu_metric_field};

pub(super) fn merge_sample(
    devices: &mut BTreeMap<String, GpuMetrics>,
    provider: ProviderId,
    sample: GpuProviderSample,
    observed_at_ms: u64,
) {
    if sample.metrics.device_id.is_empty() {
        return;
    }
    let device_id = sample.metrics.device_id.clone();
    let target = devices.entry(device_id.clone()).or_insert_with(|| {
        let mut metrics = GpuMetrics::new(device_id, "");
        metrics.device_state = sample.metrics.device_state;
        metrics
    });
    let had_engine_owner = target
        .provenance
        .iter()
        .any(|item| item.field == GpuMetricField::Engines);
    let supplies_engines = sample.fields.contains(&GpuMetricField::Engines);
    let engine_failure = sample
        .field_failures
        .iter()
        .filter(|receipt| receipt.field == GpuMetricField::Engines)
        .map(|receipt| receipt.failure)
        .fold(None, |current, candidate| {
            super::super::preferred_gpu_failure(current, Some(candidate))
        });
    target.device_state = target
        .device_state
        .merge_observation(sample.metrics.device_state, observed_at_ms);
    let mut observations = *target.scalar_observations();
    let mut throttle_observation = target.throttle_observation().clone();

    for field in sample.fields {
        apply_gpu_metric_field(
            target,
            &sample.metrics,
            field,
            &mut observations,
            &mut throttle_observation,
        );
        if let Some(existing) = target
            .provenance
            .iter_mut()
            .find(|item| item.field == field)
        {
            existing.provider = provider.clone();
        } else {
            target.provenance.push(GpuMetricProvenance {
                field,
                provider: provider.clone(),
            });
        }
    }
    target.apply_scalar_observations(observations);
    target.apply_throttle_observation(throttle_observation);

    // Preserve the engine capability receipt in the merged device model. A
    // successful higher-priority engine field owns the value and clears an
    // older failure. A failure-only sample is allowed to replace the receipt
    // only when no lower-priority provider already owns that field; this is
    // the same ownership rule used by `merge_sample_receipts` for scalars.
    if supplies_engines {
        target.engine_failure = engine_failure;
        target.engine_provider = engine_failure.map(|_| provider.clone());
    } else if let Some(failure) = engine_failure
        && !had_engine_owner
    {
        target.engine_failure = Some(failure);
        target.engine_provider = Some(provider);
    }
}

#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", allow(dead_code))]
pub(in crate::engine::hardware::gpu) fn merge_provider_samples(
    metrics: &mut Vec<GpuMetrics>,
    provider: ProviderId,
    samples: Vec<GpuProviderSample>,
) {
    let mut devices = std::mem::take(metrics)
        .into_iter()
        .map(|metric| (metric.device_id.clone(), metric))
        .collect::<BTreeMap<_, _>>();
    for sample in samples {
        merge_sample(&mut devices, provider.clone(), sample, 0);
    }
    *metrics = devices.into_values().collect();
}
