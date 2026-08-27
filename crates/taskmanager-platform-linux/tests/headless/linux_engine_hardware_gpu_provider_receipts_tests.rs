use taskmanager_core::{GpuMetricProvenance, ProviderId};

use super::super::super::GpuProviderFieldFailure;
use super::*;

#[test]
fn lower_current_owner_is_not_degraded_by_unused_higher_provider_failure() {
    let device_id = "gpu:pci:0000:01:00.0";
    let mut metric = GpuMetrics::new(device_id, "Fixture GPU");
    metric.provenance = vec![GpuMetricProvenance {
        field: GpuMetricField::Utilization,
        provider: ProviderId::borrowed("fixture.lower"),
    }];
    let devices = BTreeMap::from([(device_id.to_owned(), metric)]);
    let sample = GpuProviderSample {
        metrics: GpuMetrics::new(device_id, "Fixture GPU"),
        fields: Vec::new(),
        field_failures: vec![GpuProviderFieldFailure {
            field: GpuMetricField::Utilization,
            failure: FailureKind::PermissionDenied,
        }],
    };
    let mut failures = GpuFieldFailures::new();

    merge_sample_receipts(&devices, &mut failures, &sample);

    assert!(failures.get(device_id).is_some_and(BTreeMap::is_empty));
}
