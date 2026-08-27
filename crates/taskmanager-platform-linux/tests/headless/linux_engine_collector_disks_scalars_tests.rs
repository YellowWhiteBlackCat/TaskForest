use taskmanager_core::{FailureKind, ScalarAvailability, ScalarObservation};

use super::*;

fn disk(generation: u64, iops: ScalarObservation<u64>) -> DiskMetrics {
    taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id("disk:wwid:fixture".into())
        .device_generation(DeviceGeneration::new(generation))
        .scalar_observations(DiskScalarObservations {
            iops,
            ..Default::default()
        })
        .build()
}

#[test]
fn failure_retains_last_success_only_inside_one_generation() {
    let mut state = DiskScalarState::default();
    let mut initial = vec![disk(1, ScalarObservation::available(7, 10))];
    state.reconcile(&mut initial);

    let mut failed = vec![disk(
        1,
        ScalarObservation::unavailable(FailureKind::PermissionDenied),
    )];
    state.reconcile(&mut failed);
    assert_eq!(
        failed[0].scalar_observations().iops.availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(
        failed[0]
            .scalar_observations()
            .iops
            .last_known_value()
            .copied(),
        Some(7)
    );
    assert_eq!(
        failed[0].scalar_observations().iops.last_success_ms(),
        Some(10)
    );

    let mut reattached = vec![disk(
        2,
        ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
    )];
    state.reconcile(&mut reattached);
    assert_eq!(
        reattached[0].scalar_observations().iops.availability(),
        ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        reattached[0].scalar_observations().iops.last_known_value(),
        None
    );
}

#[test]
fn lifecycle_reset_discards_all_prior_generations_for_stable_id() {
    let mut state = DiskScalarState::default();
    state.reconcile(&mut [disk(1, ScalarObservation::available(1, 10))]);
    state.reconcile(&mut [disk(2, ScalarObservation::available(2, 20))]);

    state.reset_generations(&[DeviceId::new("disk:wwid:fixture")]);

    let mut failed = vec![disk(
        3,
        ScalarObservation::unavailable(FailureKind::ProviderFault),
    )];
    state.reconcile(&mut failed);
    assert_eq!(
        failed[0].scalar_observations().iops.last_known_value(),
        None
    );
}
