use std::collections::{BTreeMap, HashMap};

use super::*;

fn lifecycle(
    presence: DevicePresence,
    status: DeviceStatus,
    generation: u64,
    now_ms: u64,
) -> DeviceLifecycle {
    DeviceLifecycle {
        presence,
        state: DeviceState {
            status,
            last_success_ms: (status == DeviceStatus::Healthy).then_some(now_ms),
        },
        generation,
        first_seen_ms: Some(10),
        last_seen_ms: Some(now_ms),
        absent_since_ms: (presence == DevicePresence::Absent).then_some(now_ms),
    }
}
fn storage_observation(
    entries: impl IntoIterator<Item = (&'static str, DeviceLifecycle)>,
) -> StorageTelemetryObservation {
    StorageTelemetryObservation::current(
        Vec::new(),
        10,
        Vec::new(),
        Vec::new(),
        entries
            .into_iter()
            .map(|(stable_id, lifecycle)| (DeviceId::new(stable_id), lifecycle))
            .collect(),
    )
}

fn storage_observation_owned(
    entries: impl IntoIterator<Item = (String, DeviceLifecycle)>,
) -> StorageTelemetryObservation {
    StorageTelemetryObservation::current(
        Vec::new(),
        10,
        Vec::new(),
        Vec::new(),
        entries
            .into_iter()
            .map(|(stable_id, lifecycle)| (DeviceId::new(stable_id), lifecycle))
            .collect(),
    )
}

fn sensor_snapshot(
    entries: impl IntoIterator<Item = (&'static str, DeviceLifecycle)>,
) -> SensorCenterSnapshot {
    SensorCenterSnapshot {
        device_lifecycles: sidecar(entries),
        ..SensorCenterSnapshot::default()
    }
}

fn power_snapshot(
    entries: impl IntoIterator<Item = (&'static str, DeviceLifecycle)>,
) -> PowerSupplySnapshot {
    PowerSupplySnapshot {
        device_lifecycles: sidecar(entries),
        ..PowerSupplySnapshot::default()
    }
}

fn sidecar(
    entries: impl IntoIterator<Item = (&'static str, DeviceLifecycle)>,
) -> HashMap<String, DeviceLifecycle> {
    entries
        .into_iter()
        .map(|(stable_id, lifecycle)| (stable_id.to_string(), lifecycle))
        .collect()
}

fn empty_storage_observation() -> StorageTelemetryObservation {
    storage_observation(std::iter::empty())
}

fn applied(result: DeviceLifecycleApplyResult) -> DeviceLifecycleProjectionDelta {
    let DeviceLifecycleApplyResult::Applied(delta) = result else {
        panic!("snapshot should be applied")
    };
    delta
}

#[test]
fn duplicate_conflicting_and_out_of_order_snapshots_are_ignored() {
    let mut projection = DeviceLifecycleProjection::default();
    let first = storage_observation([(
        "device:stable:a",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
    )]);
    applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(5), &first),
    );

    assert_eq!(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(5), &first),
        DeviceLifecycleApplyResult::Ignored(DeviceLifecycleSnapshotRejection::Duplicate {
            partition: DeviceLifecyclePartition::SystemStorage,
            revision: DeviceLifecycleSnapshotRevision::new(5)
        })
    );
    let conflict = storage_observation([(
        "device:stable:a",
        lifecycle(DevicePresence::Absent, DeviceStatus::Stale, 1, 11),
    )]);
    assert_eq!(
        projection.apply_storage_telemetry_observation(
            DeviceLifecycleSnapshotRevision::new(5),
            &conflict
        ),
        DeviceLifecycleApplyResult::Ignored(
            DeviceLifecycleSnapshotRejection::ConflictingDuplicate {
                partition: DeviceLifecyclePartition::SystemStorage,
                revision: DeviceLifecycleSnapshotRevision::new(5)
            }
        )
    );
    assert_eq!(
        projection.apply_storage_telemetry_observation(
            DeviceLifecycleSnapshotRevision::new(4),
            &conflict
        ),
        DeviceLifecycleApplyResult::Ignored(DeviceLifecycleSnapshotRejection::OutOfOrder {
            partition: DeviceLifecyclePartition::SystemStorage,
            accepted: DeviceLifecycleSnapshotRevision::new(5),
            received: DeviceLifecycleSnapshotRevision::new(4),
        })
    );
    assert_eq!(
        projection.get("device:stable:a").map(|device| device.state),
        Some(DeviceLifecycleViewState::Present)
    );
}

#[test]
fn provider_outage_never_becomes_disconnect_or_removal() {
    let mut projection = DeviceLifecycleProjection::default();
    let present = storage_observation([(
        "device:stable:a",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
    )]);
    applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(1), &present),
    );
    let unavailable = storage_observation([(
        "device:stable:a",
        lifecycle(
            DevicePresence::Unavailable,
            DeviceStatus::PermissionDenied,
            1,
            20,
        ),
    )]);
    let unavailable_delta = applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(2),
        &unavailable,
    ));
    assert_eq!(
        unavailable_delta.changes[0].kind,
        DeviceLifecycleChangeKind::ProviderUnavailable
    );

    let missing = empty_storage_observation();
    let missing_delta = applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(3), &missing),
    );
    assert!(missing_delta.changes.is_empty());
    assert!(matches!(
        missing_delta.issues.as_slice(),
        [DeviceLifecycleProjectionIssue::MissingWithoutConfirmedDisconnect { .. }]
    ));
    assert_eq!(
        projection.get("device:stable:a").map(|device| device.state),
        Some(DeviceLifecycleViewState::ProviderUnavailable(
            DeviceStatus::PermissionDenied
        ))
    );

    let recovered_delta = applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(4), &present),
    );
    assert_eq!(
        recovered_delta.changes[0].kind,
        DeviceLifecycleChangeKind::ProviderRecovered
    );
    assert_eq!(
        projection
            .get("device:stable:a")
            .map(|device| device.generation.get()),
        Some(1)
    );
}

#[test]
fn confirmed_disconnect_reappearance_and_expiry_are_typed() {
    let mut projection = DeviceLifecycleProjection::default();
    let present = storage_observation([(
        "device:stable:a",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
    )]);
    applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(1), &present),
    );
    let disconnected = storage_observation([(
        "device:stable:a",
        lifecycle(DevicePresence::Absent, DeviceStatus::Stale, 1, 20),
    )]);
    let disconnected_delta = applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(2),
        &disconnected,
    ));
    assert_eq!(
        disconnected_delta.changes[0].kind,
        DeviceLifecycleChangeKind::Disconnected
    );

    let returned = storage_observation([(
        "device:stable:a",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 2, 30),
    )]);
    let returned_delta =
        applied(projection.apply_storage_telemetry_observation(
            DeviceLifecycleSnapshotRevision::new(3),
            &returned,
        ));
    assert_eq!(
        returned_delta.changes[0].kind,
        DeviceLifecycleChangeKind::Reappeared
    );
    assert_eq!(
        returned_delta.changes[0].current_generation,
        Some(DeviceGeneration::new(2))
    );

    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(4),
        &storage_observation([(
            "device:stable:a",
            lifecycle(DevicePresence::Absent, DeviceStatus::Stale, 2, 40),
        )]),
    ));
    let removed = applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(5),
        &empty_storage_observation(),
    ));
    assert_eq!(removed.changes[0].kind, DeviceLifecycleChangeKind::Removed);
    assert_eq!(removed.changes[0].current, None);
    assert!(projection.get("device:stable:a").is_none());
}

#[test]
fn reorder_is_inert_and_generation_regression_is_retained_as_an_issue() {
    let mut projection = DeviceLifecycleProjection::default();
    let initial = storage_observation([
        (
            "device:stable:b",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 2, 10),
        ),
        (
            "device:stable:a",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
        ),
    ]);
    applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(1), &initial),
    );
    let reordered = storage_observation([
        (
            "device:stable:a",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
        ),
        (
            "device:stable:b",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 2, 10),
        ),
    ]);
    let inert =
        applied(projection.apply_storage_telemetry_observation(
            DeviceLifecycleSnapshotRevision::new(2),
            &reordered,
        ));
    assert!(inert.changes.is_empty());

    let regressed = storage_observation([
        (
            "device:stable:a",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 20),
        ),
        (
            "device:stable:b",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 20),
        ),
    ]);
    let regression =
        applied(projection.apply_storage_telemetry_observation(
            DeviceLifecycleSnapshotRevision::new(3),
            &regressed,
        ));
    assert!(matches!(
        regression.issues.as_slice(),
        [DeviceLifecycleProjectionIssue::GenerationRegressed {
            retained,
            observed,
            ..
        }] if *retained == DeviceGeneration::new(2)
            && *observed == DeviceGeneration::new(1)
    ));
    assert_eq!(
        projection
            .get("device:stable:b")
            .map(|device| device.generation),
        Some(DeviceGeneration::new(2))
    );
    assert_eq!(
        projection
            .devices()
            .map(|device| device.stable_id.as_str())
            .collect::<Vec<_>>(),
        ["device:stable:a", "device:stable:b"]
    );
}

#[test]
fn zero_generation_is_not_promoted_into_a_trustworthy_device_row() {
    let mut projection = DeviceLifecycleProjection::default();
    let delta = applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(1),
        &storage_observation([(
            "device:stable:a",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 0, 10),
        )]),
    ));
    assert!(matches!(
        delta.issues.as_slice(),
        [DeviceLifecycleProjectionIssue::ZeroGeneration { .. }]
    ));
    assert!(projection.devices().next().is_none());
}

#[test]
fn zero_generation_observation_retains_existing_row_without_missing_or_removal() {
    let mut projection = DeviceLifecycleProjection::default();
    let present = storage_observation([(
        "opaque-retained",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
    )]);
    applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(1), &present),
    );

    let invalid = storage_observation([(
        "opaque-retained",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 0, 20),
    )]);
    let delta = applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(2), &invalid),
    );

    assert!(delta.changes.is_empty());
    assert!(matches!(
        delta.issues.as_slice(),
        [DeviceLifecycleProjectionIssue::ZeroGeneration { stable_id }]
            if stable_id.as_str() == "opaque-retained"
    ));
    assert_eq!(
        projection
            .get("opaque-retained")
            .map(|device| device.generation),
        Some(DeviceGeneration::new(1))
    );
}

#[test]
fn diagnostic_history_is_bounded_and_retains_ignored_revisions() {
    let mut history = DeviceLifecycleDiagnosticHistory::default();
    for revision in 1..=(DEVICE_LIFECYCLE_DIAGNOSTIC_CAPACITY as u64 + 1) {
        history.record(DeviceLifecycleApplyResult::Ignored(
            DeviceLifecycleSnapshotRejection::Duplicate {
                partition: DeviceLifecyclePartition::SystemStorage,
                revision: DeviceLifecycleSnapshotRevision::new(revision),
            },
        ));
    }

    assert_eq!(history.len(), DEVICE_LIFECYCLE_DIAGNOSTIC_CAPACITY);
    assert!(matches!(
        history.entries().next(),
        Some(DeviceLifecycleApplyResult::Ignored(
            DeviceLifecycleSnapshotRejection::Duplicate {
                partition: DeviceLifecyclePartition::SystemStorage,
                revision,
            }
        )) if revision.get() == 2
    ));
    assert!(matches!(
        history.latest(),
        Some(DeviceLifecycleApplyResult::Ignored(
            DeviceLifecycleSnapshotRejection::Duplicate {
                partition: DeviceLifecyclePartition::SystemStorage,
                revision,
            }
        )) if revision.get() == DEVICE_LIFECYCLE_DIAGNOSTIC_CAPACITY as u64 + 1
    ));
}

#[test]
fn diagnostic_history_retains_non_fatal_projection_issues() {
    let mut history = DeviceLifecycleDiagnosticHistory::default();
    history.record(DeviceLifecycleApplyResult::Applied(
        DeviceLifecycleProjectionDelta {
            partition: DeviceLifecyclePartition::SystemStorage,
            revision: DeviceLifecycleSnapshotRevision::new(9),
            changes: Vec::new(),
            issues: vec![DeviceLifecycleProjectionIssue::EmptyStableId],
        },
    ));

    assert!(matches!(
        history.latest(),
        Some(DeviceLifecycleApplyResult::Applied(
            DeviceLifecycleProjectionDelta { issues, .. }
        )) if matches!(
            issues.as_slice(),
            [DeviceLifecycleProjectionIssue::EmptyStableId]
        )
    ));
}

#[test]
fn partition_refresh_only_reconciles_devices_owned_by_that_partition() {
    let mut projection = DeviceLifecycleProjection::default();
    let system = storage_observation([(
        "opaque-system",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
    )]);
    let sensors = sensor_snapshot([(
        "opaque-sensor",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
    )]);
    applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(1), &system),
    );
    applied(projection.apply_sensor_snapshot(DeviceLifecycleSnapshotRevision::new(1), &sensors));

    let disconnected = storage_observation([(
        "opaque-system",
        lifecycle(DevicePresence::Absent, DeviceStatus::Stale, 1, 20),
    )]);
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(2),
        &disconnected,
    ));
    let removed = applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(3),
        &empty_storage_observation(),
    ));

    assert_eq!(removed.partition, DeviceLifecyclePartition::SystemStorage);
    assert_eq!(removed.changes.len(), 1);
    assert_eq!(removed.changes[0].stable_id.as_str(), "opaque-system");
    assert!(removed.issues.is_empty());
    assert!(projection.get("opaque-system").is_none());
    assert_eq!(
        projection
            .get("opaque-sensor")
            .map(|device| device.partition),
        Some(DeviceLifecyclePartition::Sensors)
    );
}

#[test]
fn revisions_and_duplicate_detection_are_partition_local() {
    let mut projection = DeviceLifecycleProjection::default();
    let system = storage_observation([(
        "opaque-system",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
    )]);
    let sensors = sensor_snapshot([(
        "opaque-sensor",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
    )]);
    let power = power_snapshot([(
        "opaque-power",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
    )]);

    applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(10), &system),
    );
    applied(projection.apply_sensor_snapshot(DeviceLifecycleSnapshotRevision::new(2), &sensors));
    applied(
        projection.apply_power_supply_snapshot(DeviceLifecycleSnapshotRevision::new(1), &power),
    );

    assert_eq!(
        projection.accepted_revision_for(DeviceLifecyclePartition::SystemStorage),
        Some(DeviceLifecycleSnapshotRevision::new(10))
    );
    assert_eq!(
        projection.accepted_revision_for(DeviceLifecyclePartition::Sensors),
        Some(DeviceLifecycleSnapshotRevision::new(2))
    );
    assert_eq!(
        projection.apply_sensor_snapshot(DeviceLifecycleSnapshotRevision::new(1), &sensors),
        DeviceLifecycleApplyResult::Ignored(DeviceLifecycleSnapshotRejection::OutOfOrder {
            partition: DeviceLifecyclePartition::Sensors,
            accepted: DeviceLifecycleSnapshotRevision::new(2),
            received: DeviceLifecycleSnapshotRevision::new(1),
        })
    );
    assert_eq!(
        projection.apply_power_supply_snapshot(DeviceLifecycleSnapshotRevision::new(1), &power,),
        DeviceLifecycleApplyResult::Ignored(DeviceLifecycleSnapshotRejection::Duplicate {
            partition: DeviceLifecyclePartition::PowerSupplies,
            revision: DeviceLifecycleSnapshotRevision::new(1),
        })
    );
}

#[test]
fn first_partition_keeps_global_authority_on_id_conflict_and_after_removal() {
    let mut projection = DeviceLifecycleProjection::default();
    let system = storage_observation([(
        "opaque-shared",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
    )]);
    applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(1), &system),
    );

    let conflicting_sensor = sensor_snapshot([(
        "opaque-shared",
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 7, 20),
    )]);
    let conflict = applied(
        projection
            .apply_sensor_snapshot(DeviceLifecycleSnapshotRevision::new(1), &conflicting_sensor),
    );
    assert!(conflict.changes.is_empty());
    assert!(matches!(
        conflict.issues.as_slice(),
        [DeviceLifecycleProjectionIssue::OwnershipConflict {
            stable_id,
            authoritative_partition: DeviceLifecyclePartition::SystemStorage,
            observed_partition: DeviceLifecyclePartition::Sensors,
        }] if stable_id.as_str() == "opaque-shared"
    ));
    assert_eq!(
        projection.authority("opaque-shared"),
        Some(DeviceLifecyclePartition::SystemStorage)
    );
    assert_eq!(
        projection
            .get("opaque-shared")
            .map(|device| device.generation.get()),
        Some(1)
    );

    let disconnected = storage_observation([(
        "opaque-shared",
        lifecycle(DevicePresence::Absent, DeviceStatus::Stale, 1, 30),
    )]);
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(2),
        &disconnected,
    ));
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(3),
        &empty_storage_observation(),
    ));
    let power_conflict = applied(projection.apply_power_supply_snapshot(
        DeviceLifecycleSnapshotRevision::new(1),
        &power_snapshot([(
            "opaque-shared",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 40),
        )]),
    ));
    assert!(matches!(
        power_conflict.issues.as_slice(),
        [DeviceLifecycleProjectionIssue::OwnershipConflict {
            authoritative_partition: DeviceLifecyclePartition::SystemStorage,
            observed_partition: DeviceLifecyclePartition::PowerSupplies,
            ..
        }]
    ));
    assert!(projection.get("opaque-shared").is_none());
    assert_eq!(
        projection.authority("opaque-shared"),
        Some(DeviceLifecyclePartition::SystemStorage)
    );
}

#[test]
fn removed_owner_tombstones_keep_only_the_bounded_newest_tail() {
    let mut projection = DeviceLifecycleProjection::default();
    let device_count = DEVICE_LIFECYCLE_OWNER_TOMBSTONE_CAPACITY + 1;
    let present = (0..device_count).map(|index| {
        (
            format!("retired-{index:04}"),
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
        )
    });
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(1),
        &storage_observation_owned(present),
    ));
    let absent = (0..device_count).map(|index| {
        (
            format!("retired-{index:04}"),
            lifecycle(DevicePresence::Absent, DeviceStatus::Stale, 1, 20),
        )
    });
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(2),
        &storage_observation_owned(absent),
    ));
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(3),
        &empty_storage_observation(),
    ));

    assert_eq!(projection.authority("retired-0000"), None);
    assert_eq!(
        projection.authority(&format!("retired-{:04}", device_count - 1)),
        Some(DeviceLifecyclePartition::SystemStorage)
    );
    assert_eq!(
        projection.retired_owner_order.len(),
        DEVICE_LIFECYCLE_OWNER_TOMBSTONE_CAPACITY
    );
}

#[test]
fn reappearing_identity_preserves_unrelated_tombstone_eviction_order() {
    let mut projection = DeviceLifecycleProjection::default();
    let initially_present = storage_observation([
        (
            "retired-older",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
        ),
        (
            "retired-target",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
        ),
    ]);
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(1),
        &initially_present,
    ));
    let disconnected = storage_observation([
        (
            "retired-older",
            lifecycle(DevicePresence::Absent, DeviceStatus::Stale, 1, 20),
        ),
        (
            "retired-target",
            lifecycle(DevicePresence::Absent, DeviceStatus::Stale, 1, 20),
        ),
    ]);
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(2),
        &disconnected,
    ));
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(3),
        &empty_storage_observation(),
    ));

    let target = || {
        (
            "retired-target".to_string(),
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 2, 30),
        )
    };
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(4),
        &storage_observation_owned([target()]),
    ));

    let present_churn = std::iter::once(target()).chain(
        (0..DEVICE_LIFECYCLE_OWNER_TOMBSTONE_CAPACITY).map(|index| {
            (
                format!("fresh-{index:04}"),
                lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 40),
            )
        }),
    );
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(5),
        &storage_observation_owned(present_churn),
    ));
    let disconnected_churn = std::iter::once(target()).chain(
        (0..DEVICE_LIFECYCLE_OWNER_TOMBSTONE_CAPACITY).map(|index| {
            (
                format!("fresh-{index:04}"),
                lifecycle(DevicePresence::Absent, DeviceStatus::Stale, 1, 50),
            )
        }),
    );
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(6),
        &storage_observation_owned(disconnected_churn),
    ));
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(7),
        &storage_observation_owned([target()]),
    ));

    assert_eq!(
        projection.authority("retired-older"),
        None,
        "the oldest unrelated tombstone must remain eligible for bounded eviction"
    );
    assert_eq!(
        projection.authority("retired-target"),
        Some(DeviceLifecyclePartition::SystemStorage),
        "a reappeared device keeps its active partition authority"
    );
}

#[test]
fn global_and_partition_views_are_sorted_by_opaque_stable_id() {
    let mut projection = DeviceLifecycleProjection::default();
    let power = power_snapshot([
        (
            "opaque-d",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
        ),
        (
            "opaque-b",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
        ),
    ]);
    let sensors = sensor_snapshot([
        (
            "opaque-c",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
        ),
        (
            "opaque-a",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
        ),
    ]);
    let power_delta = applied(
        projection.apply_power_supply_snapshot(DeviceLifecycleSnapshotRevision::new(1), &power),
    );
    applied(projection.apply_sensor_snapshot(DeviceLifecycleSnapshotRevision::new(1), &sensors));

    assert_eq!(
        power_delta
            .changes
            .iter()
            .map(|change| change.stable_id.as_str())
            .collect::<Vec<_>>(),
        ["opaque-b", "opaque-d"]
    );
    assert_eq!(
        projection
            .devices()
            .map(|device| device.stable_id.as_str())
            .collect::<Vec<_>>(),
        ["opaque-a", "opaque-b", "opaque-c", "opaque-d"]
    );
    assert_eq!(
        projection
            .devices_in_partition(DeviceLifecyclePartition::Sensors)
            .map(|device| device.stable_id.as_str())
            .collect::<Vec<_>>(),
        ["opaque-a", "opaque-c"]
    );
}

#[test]
fn typed_system_domains_are_independently_revised() {
    let mut projection = DeviceLifecycleProjection::default();
    let observed = lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10);
    let first_storage = storage_observation([("opaque-storage", observed)]);
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(1),
        &first_storage,
    ));
    let storage = StorageTelemetryObservation::current(
        Vec::new(),
        20,
        Vec::new(),
        Vec::new(),
        BTreeMap::from([(DeviceId::new("opaque-storage"), observed)]),
    );
    let network = NetworkTelemetryObservation::current(
        Vec::new(),
        20,
        Vec::new(),
        Vec::new(),
        BTreeMap::from([(DeviceId::new("opaque-network"), observed)]),
    );

    applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(2), &storage),
    );
    applied(
        projection
            .apply_network_telemetry_observation(DeviceLifecycleSnapshotRevision::new(1), &network),
    );

    assert_eq!(
        projection.authority("opaque-storage"),
        Some(DeviceLifecyclePartition::SystemStorage)
    );
    assert_eq!(
        projection.authority("opaque-network"),
        Some(DeviceLifecyclePartition::SystemNetwork)
    );
    assert_eq!(
        projection.accepted_revision_for(DeviceLifecyclePartition::SystemStorage),
        Some(DeviceLifecycleSnapshotRevision::new(2))
    );
    assert_eq!(
        projection.accepted_revision_for(DeviceLifecyclePartition::SystemNetwork),
        Some(DeviceLifecycleSnapshotRevision::new(1))
    );
}

#[test]
fn unconfirmed_missing_retention_is_bounded_and_retires_only_newcomers() {
    let mut projection = DeviceLifecycleProjection::default();
    let capacity = super::DEVICE_LIFECYCLE_UNCONFIRMED_RETENTION_CAPACITY;
    let present = storage_observation_owned(
        (0..=capacity)
            .map(|index| {
                (
                    format!("device:stable:{index}"),
                    lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
                )
            })
            .collect::<Vec<_>>(),
    );
    applied(
        projection
            .apply_storage_telemetry_observation(DeviceLifecycleSnapshotRevision::new(1), &present),
    );
    assert_eq!(projection.devices().count(), capacity + 1);

    // One provider outage with no confirmed disconnects: the first
    // `capacity` rows stay retained; the row that would cross the ceiling is
    // retired through the honest removal path in the same delta.
    let missing_delta = applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(2),
        &empty_storage_observation(),
    ));
    assert_eq!(
        missing_delta.issues.len(),
        capacity + 1,
        "every missing row still reports its unconfirmed-missing issue"
    );
    assert_eq!(
        missing_delta
            .changes
            .iter()
            .filter(|change| change.kind == DeviceLifecycleChangeKind::Removed)
            .count(),
        1,
        "exactly the ceiling-crossing newcomer is retired"
    );
    assert_eq!(projection.devices().count(), capacity);

    // A further outage snapshot must not retire or grow anything: the
    // retained rows are already accounted for.
    let steady_delta = applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(3),
        &empty_storage_observation(),
    ));
    assert!(steady_delta.changes.is_empty());
    assert_eq!(projection.devices().count(), capacity);

    // A retained row that reappears is readmitted and frees its retention
    // slot; a present device is never affected by the ceiling.
    let recovered = storage_observation_owned(vec![(
        "device:stable:0".to_string(),
        lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 2, 30),
    )]);
    let recovered_delta =
        applied(projection.apply_storage_telemetry_observation(
            DeviceLifecycleSnapshotRevision::new(4),
            &recovered,
        ));
    assert_eq!(
        recovered_delta.changes[0].kind,
        DeviceLifecycleChangeKind::Reappeared
    );
    assert_eq!(projection.devices().count(), capacity);
}
