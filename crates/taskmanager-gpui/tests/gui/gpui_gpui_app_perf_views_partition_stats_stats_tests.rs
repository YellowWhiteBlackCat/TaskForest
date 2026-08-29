use taskmanager_core::core::FailureKind;
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::metrics::{DiskPartitionScalarObservations, ScalarObservation};

use super::{PartitionUsage, partition_usage};

#[test]
fn partition_usage_distinguishes_current_mounted_data_from_unavailable_space() {
    let mut mounted = taskmanager_test_support::DiskPartitionFixtureBuilder::new()
        .device_state(DeviceState::healthy(10))
        .mount_point("/data".into())
        .build();
    mounted.apply_scalar_observations(DiskPartitionScalarObservations {
        capacity_bytes: ScalarObservation::available(1_000, 10),
        used_bytes: ScalarObservation::available(400, 10),
        free_bytes: ScalarObservation::available(600, 10),
    });
    assert_eq!(
        partition_usage(&mounted),
        PartitionUsage::Current {
            used: 400,
            free: 600,
            total: 1_000,
        }
    );

    let mut unmounted = mounted.clone();
    unmounted.mount_point.clear();
    unmounted.apply_scalar_observations(DiskPartitionScalarObservations {
        capacity_bytes: ScalarObservation::available(1_000, 10),
        ..DiskPartitionScalarObservations::unavailable(FailureKind::Unsupported)
    });
    assert_eq!(
        partition_usage(&unmounted),
        PartitionUsage::Unavailable(DeviceStatus::Healthy)
    );

    unmounted.device_state =
        DeviceState::healthy(10).transition(DeviceStatus::PermissionDenied, 20);
    assert_eq!(
        partition_usage(&unmounted),
        PartitionUsage::Unavailable(DeviceStatus::PermissionDenied)
    );
}

#[test]
fn partially_available_partition_observations_do_not_fold_into_current_usage() {
    // Capacity is observable for an unmounted partition while used/free
    // space is not — the fold must stay Unavailable rather than invent
    // zeros, and a fully-observed sibling of the same shape folds Current.
    let mut partial = taskmanager_test_support::DiskPartitionFixtureBuilder::new()
        .device_state(DeviceState::healthy(10))
        .mount_point(String::new())
        .build();
    partial.apply_scalar_observations(DiskPartitionScalarObservations {
        capacity_bytes: ScalarObservation::available(2_048, 10),
        ..DiskPartitionScalarObservations::unavailable(FailureKind::Unsupported)
    });
    assert_eq!(
        partition_usage(&partial),
        PartitionUsage::Unavailable(DeviceStatus::Healthy)
    );

    let mut full = partial.clone();
    full.mount_point = "/data".into();
    full.apply_scalar_observations(DiskPartitionScalarObservations {
        capacity_bytes: ScalarObservation::available(2_048, 10),
        used_bytes: ScalarObservation::available(512, 10),
        free_bytes: ScalarObservation::available(1_536, 10),
    });
    assert_eq!(
        partition_usage(&full),
        PartitionUsage::Current {
            used: 512,
            free: 1_536,
            total: 2_048,
        }
    );
}
