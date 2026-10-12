//! test-intent: behavior

use super::*;

#[test]
fn npu_generation_advances_only_after_a_confirmed_absence() {
    let mut projection = DeviceLifecycleProjection::default();
    let npu = |id: &str| NpuDevice {
        device_id: DeviceId::new(id),
        ..NpuDevice::default()
    };

    let mut first = NpuInventorySnapshot::discovered(vec![npu("npu:0")], 10);
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(1), &mut first),
    );
    assert_eq!(
        first.devices[0].device_generation,
        DeviceGeneration::INITIAL
    );
    assert_eq!(
        projection.authority("npu:0"),
        Some(DeviceLifecyclePartition::NpuInventory)
    );

    let mut absent = NpuInventorySnapshot::discovered(Vec::new(), 20);
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(2), &mut absent),
    );
    assert_eq!(
        projection.get("npu:0").map(|device| device.state),
        Some(DeviceLifecycleViewState::Disconnected)
    );

    let mut returned = NpuInventorySnapshot::discovered(vec![npu("npu:0")], 30);
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(3), &mut returned),
    );
    assert_eq!(returned.devices[0].device_generation.get(), 2);
}

#[test]
fn npu_generation_survives_the_application_absence_retention_window() {
    let mut projection = DeviceLifecycleProjection::default();
    let npu = |id: &str| NpuDevice {
        device_id: DeviceId::new(id),
        ..NpuDevice::default()
    };

    let mut first = NpuInventorySnapshot::discovered(vec![npu("npu:alpha")], 10);
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(1), &mut first),
    );
    let mut absent = NpuInventorySnapshot::discovered(Vec::new(), 20);
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(2), &mut absent),
    );

    let mut expired = NpuInventorySnapshot::discovered(Vec::new(), 30_021);
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(3), &mut expired),
    );
    assert!(projection.get("npu:alpha").is_none());

    let mut returned =
        NpuInventorySnapshot::discovered(vec![npu("npu:alpha"), npu("npu:beta")], 30_030);
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(4), &mut returned),
    );
    assert_eq!(
        returned.devices[0].device_generation,
        DeviceGeneration::new(2)
    );
    assert_eq!(
        returned.devices[1].device_generation,
        DeviceGeneration::INITIAL
    );
    assert_eq!(
        projection.get("npu:alpha").map(|device| device.generation),
        Some(DeviceGeneration::new(2))
    );
}

#[test]
fn npu_inventory_failure_preserves_generation_without_confirming_absence() {
    let mut projection = DeviceLifecycleProjection::default();
    let mut first = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("npu:0"),
            ..NpuDevice::default()
        }],
        10,
    );
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(1), &mut first),
    );

    let mut failed = NpuInventorySnapshot::failed(FailureKind::ProviderFault, "unavailable", 20);
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(2), &mut failed),
    );
    assert_eq!(
        projection
            .get("npu:0")
            .map(|device| device.generation.get()),
        Some(1)
    );
    assert_eq!(
        projection.get("npu:0").map(|device| device.state),
        Some(DeviceLifecycleViewState::ProviderUnavailable(
            DeviceStatus::Stale
        ))
    );

    let mut recovered = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("npu:0"),
            ..NpuDevice::default()
        }],
        30,
    );
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(3), &mut recovered),
    );
    assert_eq!(recovered.devices[0].device_generation.get(), 1);
}

#[test]
fn empty_npu_id_makes_inventory_incomplete_until_a_clean_empty_enumeration() {
    let mut projection = DeviceLifecycleProjection::default();
    let npu = |id: &str| NpuDevice {
        device_id: DeviceId::new(id),
        ..NpuDevice::default()
    };
    let mut known = NpuInventorySnapshot::discovered(vec![npu("npu:known")], 10);
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(1), &mut known),
    );

    let mut identityless = NpuInventorySnapshot::discovered(vec![npu("")], 20);
    let delta =
        applied(projection.apply_npu_inventory_snapshot(
            DeviceLifecycleSnapshotRevision::new(2),
            &mut identityless,
        ));
    assert_eq!(
        delta.issues,
        [DeviceLifecycleProjectionIssue::EmptyStableId]
    );
    assert_eq!(
        identityless.devices[0].device_generation,
        DeviceGeneration::default()
    );
    assert_eq!(
        projection
            .get("npu:known")
            .map(|device| device.generation.get()),
        Some(1)
    );
    assert_eq!(
        projection.get("npu:known").map(|device| device.state),
        Some(DeviceLifecycleViewState::ProviderUnavailable(
            DeviceStatus::Stale
        ))
    );

    let mut clean_empty = NpuInventorySnapshot::discovered(Vec::new(), 30);
    applied(
        projection.apply_npu_inventory_snapshot(
            DeviceLifecycleSnapshotRevision::new(3),
            &mut clean_empty,
        ),
    );
    assert_eq!(
        projection.get("npu:known").map(|device| device.state),
        Some(DeviceLifecycleViewState::Disconnected)
    );
    assert_eq!(
        projection
            .get("npu:known")
            .map(|device| device.generation.get()),
        Some(1)
    );
}

#[test]
fn duplicate_npu_ids_have_no_stable_identity_and_do_not_confirm_absence() {
    let mut projection = DeviceLifecycleProjection::default();
    let npu = |id: &str| NpuDevice {
        device_id: DeviceId::new(id),
        ..NpuDevice::default()
    };
    let mut first = NpuInventorySnapshot::discovered(vec![npu("npu:duplicate")], 10);
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(1), &mut first),
    );

    let mut duplicate = NpuInventorySnapshot::discovered(
        vec![npu("npu:duplicate"), npu("npu:duplicate"), npu("npu:other")],
        20,
    );
    let delta = applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(2), &mut duplicate),
    );
    assert!(delta.issues.iter().any(|issue| matches!(
        issue,
        DeviceLifecycleProjectionIssue::DuplicateStableId { stable_id }
            if stable_id.as_str() == "npu:duplicate"
    )));
    assert_eq!(
        duplicate
            .devices
            .iter()
            .filter(|device| device.device_id.as_str() == "npu:duplicate")
            .map(|device| device.device_generation)
            .collect::<Vec<_>>(),
        [DeviceGeneration::default(), DeviceGeneration::default()]
    );
    assert_eq!(
        duplicate
            .devices
            .iter()
            .find(|device| device.device_id.as_str() == "npu:other")
            .map(|device| device.device_generation.get()),
        Some(1)
    );
    assert_eq!(
        projection
            .get("npu:duplicate")
            .map(|device| device.generation.get()),
        Some(1)
    );
    assert_eq!(
        projection.get("npu:duplicate").map(|device| device.state),
        Some(DeviceLifecycleViewState::ProviderUnavailable(
            DeviceStatus::Stale
        ))
    );

    let mut clean = NpuInventorySnapshot::discovered(vec![npu("npu:duplicate")], 30);
    applied(
        projection
            .apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(3), &mut clean),
    );
    assert_eq!(clean.devices[0].device_generation.get(), 1);
}

#[test]
fn npu_cannot_claim_a_device_id_owned_by_another_lifecycle_partition() {
    let mut projection = DeviceLifecycleProjection::default();
    applied(projection.apply_storage_telemetry_observation(
        DeviceLifecycleSnapshotRevision::new(1),
        &storage_observation([(
            "shared:device",
            lifecycle(DevicePresence::Present, DeviceStatus::Healthy, 1, 10),
        )]),
    ));

    let mut npu = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("shared:device"),
            ..NpuDevice::default()
        }],
        20,
    );
    let delta = applied(
        projection.apply_npu_inventory_snapshot(DeviceLifecycleSnapshotRevision::new(1), &mut npu),
    );

    assert!(matches!(
        delta.issues.as_slice(),
        [DeviceLifecycleProjectionIssue::OwnershipConflict {
            stable_id,
            authoritative_partition: DeviceLifecyclePartition::SystemStorage,
            observed_partition: DeviceLifecyclePartition::NpuInventory,
        }] if stable_id.as_str() == "shared:device"
    ));
    assert_eq!(
        npu.devices[0].device_generation,
        DeviceGeneration::default()
    );
    assert_eq!(
        projection.authority("shared:device"),
        Some(DeviceLifecyclePartition::SystemStorage)
    );
}
