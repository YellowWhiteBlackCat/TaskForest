use taskmanager_core::{GpuMetricProvenance, ScalarAvailability};

use super::*;

#[test]
fn provider_failure_retains_each_prior_scalar_as_stale_then_recovers() {
    let device_id = "gpu:pci:0000:01:00.0";
    let provider = ProviderId::borrowed("fixture.gpu");
    let mut tracker = GpuScalarTracker::default();
    let mut first = vec![metric(device_id, provider.clone(), Some(42.0), Some(125.0))];
    tracker.observe(&mut first, &BTreeMap::new(), &GpuFieldFailures::new(), 10);

    let mut failed = vec![metric(device_id, provider.clone(), None, None)];
    tracker.observe(
        &mut failed,
        &BTreeMap::from([(provider.clone(), FailureKind::PermissionDenied)]),
        &GpuFieldFailures::new(),
        20,
    );

    assert_eq!(
        failed[0]
            .scalar_observations()
            .utilization_pct
            .availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(
        failed[0]
            .scalar_observations()
            .utilization_pct
            .last_known_value(),
        Some(&42.0)
    );
    assert_eq!(failed[0].current_utilization_pct(), None);

    let mut recovered = vec![metric(device_id, provider, Some(0.0), Some(0.0))];
    tracker.observe(
        &mut recovered,
        &BTreeMap::new(),
        &GpuFieldFailures::new(),
        30,
    );
    assert_eq!(recovered[0].current_utilization_pct(), Some(0.0));
    assert_eq!(recovered[0].current_power_w(), Some(0.0));
    assert_eq!(
        recovered[0]
            .scalar_observations()
            .utilization_pct
            .last_success_ms(),
        Some(30)
    );
}

#[test]
fn invalid_provider_number_never_becomes_current_or_a_fake_zero() {
    let mut tracker = GpuScalarTracker::default();
    let mut metrics = vec![metric(
        "gpu:pci:0000:02:00.0",
        ProviderId::borrowed("fixture.gpu"),
        Some(f32::NAN),
        Some(-1.0),
    )];

    tracker.observe(&mut metrics, &BTreeMap::new(), &GpuFieldFailures::new(), 10);

    assert_eq!(metrics[0].current_utilization_pct(), None);
    assert_eq!(metrics[0].current_power_w(), None);
    assert_eq!(
        metrics[0]
            .scalar_observations()
            .utilization_pct
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
}

#[test]
fn partial_runtime_field_failure_keeps_its_exact_reason_and_other_fields_current() {
    let device_id = "gpu:pci:0000:03:00.0";
    let provider = ProviderId::borrowed("fixture.runtime");
    let mut tracker = GpuScalarTracker::default();
    let mut first = vec![metric(device_id, provider.clone(), Some(55.0), Some(120.0))];
    tracker.observe(&mut first, &BTreeMap::new(), &GpuFieldFailures::new(), 10);

    let mut partial = vec![metric(device_id, provider, None, Some(125.0))];
    let field_failures = GpuFieldFailures::from([(
        device_id.to_owned(),
        BTreeMap::from([(GpuMetricField::Utilization, FailureKind::PermissionDenied)]),
    )]);
    tracker.observe(&mut partial, &BTreeMap::new(), &field_failures, 20);

    assert_eq!(
        partial[0]
            .scalar_observations()
            .utilization_pct
            .availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(
        partial[0]
            .scalar_observations()
            .utilization_pct
            .last_success_ms(),
        Some(10)
    );
    assert_eq!(partial[0].current_utilization_pct(), None);
    assert_eq!(partial[0].current_power_w(), Some(125.0));
}

#[test]
fn generation_prune_prevents_readded_device_from_inheriting_stale_scalars() {
    let device_id = "gpu:pci:0000:04:00.0";
    let provider = ProviderId::borrowed("fixture.runtime");
    let mut tracker = GpuScalarTracker::default();
    let mut first = vec![metric(device_id, provider.clone(), Some(80.0), None)];
    tracker.observe(&mut first, &BTreeMap::new(), &GpuFieldFailures::new(), 10);

    tracker.prune(&[DeviceId::new(device_id)]);

    let mut readded = vec![metric(device_id, provider, None, None)];
    let field_failures = GpuFieldFailures::from([(
        device_id.to_owned(),
        BTreeMap::from([(
            GpuMetricField::Utilization,
            FailureKind::TemporarilyUnavailable,
        )]),
    )]);
    tracker.observe(&mut readded, &BTreeMap::new(), &field_failures, 30);

    assert_eq!(
        readded[0]
            .scalar_observations()
            .utilization_pct
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        readded[0]
            .scalar_observations()
            .utilization_pct
            .last_known_value(),
        None
    );
    assert_eq!(
        readded[0]
            .scalar_observations()
            .utilization_pct
            .last_success_ms(),
        None
    );
}

#[test]
fn typed_vram_facts_flow_through_tracker_without_crossing_memory_families() {
    let mut tracker = GpuScalarTracker::default();
    let mut metric = GpuMetrics::new("gpu:pci:0000:05:00.0", "Fixture GPU");
    metric.apply_scalar_observations(GpuScalarObservations {
        dedicated_vram_used_bytes: ScalarObservation::available(3 << 30, 0),
        dedicated_vram_total_bytes: ScalarObservation::available(8 << 30, 0),
        shared_vram_used_bytes: ScalarObservation::available(512 << 20, 0),
        shared_vram_total_bytes: ScalarObservation::available(1 << 30, 0),
        ..Default::default()
    });
    let mut metrics = vec![metric];

    tracker.observe(&mut metrics, &BTreeMap::new(), &GpuFieldFailures::new(), 10);

    let obs = metrics[0].scalar_observations();
    assert_eq!(
        obs.dedicated_vram_used_bytes.current_value(),
        Some(&(3 << 30))
    );
    assert_eq!(
        obs.dedicated_vram_total_bytes.current_value(),
        Some(&(8 << 30))
    );
    assert_eq!(
        obs.shared_vram_used_bytes.current_value(),
        Some(&(512 << 20))
    );
    assert_eq!(
        obs.shared_vram_total_bytes.current_value(),
        Some(&(1 << 30))
    );
    assert_eq!(
        metrics[0].current_dedicated_vram_used_bytes(),
        Some(3 << 30)
    );
    assert_eq!(metrics[0].current_shared_vram_total_bytes(), Some(1 << 30));
}

#[test]
fn zero_vram_sentinels_and_partial_vram_failures_preserve_only_observed_values() {
    let device_id = "gpu:pci:0000:06:00.0";
    let mut tracker = GpuScalarTracker::default();
    let mut metrics = vec![GpuMetrics::new(device_id, "Fixture GPU")];
    tracker.observe(&mut metrics, &BTreeMap::new(), &GpuFieldFailures::new(), 10);
    assert_eq!(metrics[0].current_dedicated_vram_used_bytes(), None);
    assert_eq!(metrics[0].current_dedicated_vram_total_bytes(), None);
    assert_eq!(metrics[0].current_shared_vram_used_bytes(), None);

    let mut partial = GpuMetrics::new(device_id, "Fixture GPU");
    partial.apply_scalar_observations(GpuScalarObservations {
        dedicated_vram_used_bytes: ScalarObservation::available(2 << 30, 0),
        ..Default::default()
    });
    let mut metrics = vec![partial];
    let field_failures = GpuFieldFailures::from([(
        device_id.to_owned(),
        BTreeMap::from([(GpuMetricField::DedicatedVram, FailureKind::PermissionDenied)]),
    )]);
    tracker.observe(&mut metrics, &BTreeMap::new(), &field_failures, 20);
    assert_eq!(
        metrics[0]
            .scalar_observations()
            .dedicated_vram_used_bytes
            .availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(
        metrics[0].current_dedicated_vram_used_bytes(),
        Some(2 << 30)
    );
    assert_eq!(metrics[0].current_dedicated_vram_total_bytes(), None);
    assert_eq!(
        metrics[0]
            .scalar_observations()
            .dedicated_vram_total_bytes
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
}

fn metric(
    device_id: &str,
    provider: ProviderId,
    utilization_pct: Option<f32>,
    power_w: Option<f32>,
) -> GpuMetrics {
    let mut metrics = GpuMetrics::new(device_id, "Fixture GPU");
    metrics.provenance = vec![
        GpuMetricProvenance {
            field: GpuMetricField::Utilization,
            provider: provider.clone(),
        },
        GpuMetricProvenance {
            field: GpuMetricField::Power,
            provider,
        },
    ];
    let mut observations = GpuScalarObservations::default();
    if let Some(value) = utilization_pct {
        observations.utilization_pct = ScalarObservation::available(value, 0);
    }
    if let Some(value) = power_w {
        observations.power_w = ScalarObservation::available(value, 0);
    }
    metrics.apply_scalar_observations(observations);
    metrics
}
