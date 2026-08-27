//! Per-device GPU field failure receipt merge.

use std::collections::BTreeMap;

use taskmanager_core::{FailureKind, GpuMetricField, GpuMetrics};

use super::super::GpuProviderSample;

pub(super) type GpuFieldFailures = BTreeMap<String, BTreeMap<GpuMetricField, FailureKind>>;

/// Merge one provider's partial-field receipts in provider-priority order.
///
/// A later successful owner clears a lower-priority failure. A later provider
/// failure does not degrade a current value still owned by a lower provider,
/// unless the later sample also supplies part of that field group.
pub(super) fn merge_sample_receipts(
    devices: &BTreeMap<String, GpuMetrics>,
    field_failures: &mut GpuFieldFailures,
    sample: &GpuProviderSample,
) {
    if sample.metrics.device_id.is_empty() {
        return;
    }
    let existing_fields = devices
        .get(&sample.metrics.device_id)
        .map(|metric| {
            metric
                .provenance
                .iter()
                .map(|item| item.field)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let failures = field_failures
        .entry(sample.metrics.device_id.clone())
        .or_default();
    for field in &sample.fields {
        failures.remove(field);
    }
    for failure in &sample.field_failures {
        if sample.fields.contains(&failure.field) || !existing_fields.contains(&failure.field) {
            failures.insert(failure.field, failure.failure);
        }
    }
}

#[cfg(test)]
#[path = "../../../../../tests/headless/linux_engine_hardware_gpu_provider_receipts_tests.rs"]
mod tests;
