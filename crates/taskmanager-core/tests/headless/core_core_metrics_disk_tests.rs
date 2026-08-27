use super::*;
use crate::core::ScalarAvailability;

#[test]
fn typed_disk_zero_is_current_while_failure_is_not() {
    let mut disk = DiskMetrics::new("");
    disk.apply_scalar_observations(DiskScalarObservations {
        capacity_bytes: ScalarObservation::available(0, 10),
        available_bytes: ScalarObservation::available(0, 10),
        read_bytes_per_sec: ScalarObservation::available(0, 10),
        write_bytes_per_sec: ScalarObservation::available(0, 10),
        iops: ScalarObservation::available(0, 10),
        active_time_pct: ScalarObservation::available(0.0, 10),
        response_time_ms: ScalarObservation::available(0.0, 10),
    });

    assert_eq!(disk.current_capacity_bytes(), Some(0));
    assert_eq!(disk.current_available_bytes(), Some(0));
    assert_eq!(disk.current_iops(), Some(0));
    assert_eq!(disk.current_active_time_pct(), Some(0.0));

    disk.apply_scalar_observations(DiskScalarObservations::unavailable(
        FailureKind::PermissionDenied,
    ));
    assert_eq!(disk.current_capacity_bytes(), None);
    assert_eq!(disk.current_iops(), None);
    let wire = serde_json::to_value(&disk).expect("serialize unavailable disk");
    assert!(wire.get("total_bytes").is_none());
    assert!(wire.get("iops").is_none());
}

#[test]
fn legacy_disk_values_are_used_only_while_typed_truth_is_unknown() {
    let legacy = serde_json::json!({
        "device_id": "disk:legacy:fixture",
        "name": "fixture",
        "mount_point": "/",
        "total_bytes": 100,
        "available_bytes": 0,
        "read_bytes_per_sec": 4,
        "active_time_pct": 2.5
    });
    let disk: DiskMetrics = serde_json::from_value(legacy).expect("legacy disk remains readable");

    assert_eq!(disk.current_capacity_bytes(), Some(100));
    assert_eq!(disk.current_available_bytes(), Some(0));
    assert_eq!(disk.current_read_bytes_per_sec(), Some(4));
    assert_eq!(disk.current_active_time_pct(), Some(2.5));
    assert_eq!(disk.current_iops(), None);

    let mut conflict = serde_json::to_value(&disk).expect("serialize disk");
    conflict["scalar_observations"] = serde_json::to_value(DiskScalarObservations::unavailable(
        FailureKind::TemporarilyUnavailable,
    ))
    .expect("serialize typed failure");
    let conflict: DiskMetrics = serde_json::from_value(conflict).expect("decode conflict");
    assert_eq!(conflict.current_capacity_bytes(), None);
    assert_eq!(conflict.current_read_bytes_per_sec(), None);
}

#[test]
fn same_generation_failure_retains_last_success_only_as_stale() {
    let previous = DiskScalarObservations {
        iops: ScalarObservation::available(7, 20),
        ..Default::default()
    };
    let current = DiskScalarObservations {
        iops: ScalarObservation::unavailable(FailureKind::ProviderFault),
        ..Default::default()
    }
    .retain_previous(previous);

    assert_eq!(
        current.iops.availability(),
        ScalarAvailability::Stale(FailureKind::ProviderFault)
    );
    assert_eq!(current.iops.current_value(), None);
    assert_eq!(current.iops.last_known_value(), Some(&7));
    assert_eq!(current.iops.last_success_ms(), Some(20));
}

#[test]
fn pre_migration_wire_payload_decodes_as_unknown_compatibility_truth() {
    let value = serde_json::json!({
        "device_id": "disk:legacy:fixture",
        "name": "fixture",
        "mount_point": "/",
        "total_bytes": 100,
        "available_bytes": 25,
        "read_bytes_per_sec": 4
    });
    let decoded: DiskMetrics = serde_json::from_value(value).expect("old payload remains readable");

    assert_eq!(decoded.current_capacity_bytes(), Some(100));
    assert_eq!(decoded.current_available_bytes(), Some(25));
    assert_eq!(decoded.current_read_bytes_per_sec(), Some(4));
}

#[test]
fn partition_space_keeps_unmounted_free_and_used_unknown() {
    let mut partition = DiskPartition::new("nvme0n1p1");
    partition.device_id = DiskPartition::stable_id("disk:wwid:fixture", "nvme0n1p1");
    partition.parent_device_id = "disk:wwid:fixture".into();
    partition.apply_scalar_observations(DiskPartitionScalarObservations {
        capacity_bytes: ScalarObservation::available(100, 10),
        used_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
        free_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
    });

    assert_eq!(partition.current_capacity_bytes(), Some(100));
    assert_eq!(partition.current_used_bytes(), None);
    assert_eq!(partition.current_free_bytes(), None);
}

#[test]
fn partition_projection_follows_parent_identity_and_generation() {
    let mut partition = DiskPartition::new("nvme0n1p1");
    partition.device_id = "partition:disk:wwid:old:nvme0n1p1".into();
    partition.parent_device_id = "disk:wwid:old".into();
    partition.device_generation = DeviceGeneration::new(1);
    let mut disk = DiskMetrics::new("");
    disk.device_id = "disk:wwid:new".into();
    disk.device_generation = DeviceGeneration::new(3);
    disk.partitions = vec![partition];

    disk.project_partition_lifecycle();

    assert_eq!(
        disk.partitions[0].device_id,
        "partition:disk:wwid:new:nvme0n1p1"
    );
    assert_eq!(disk.partitions[0].parent_device_id, "disk:wwid:new");
    assert_eq!(disk.partitions[0].device_generation.get(), 3);
}

#[test]
fn legacy_disk_sentinel_zero_does_not_become_success() {
    let decoded: DiskMetrics = serde_json::from_value(serde_json::json!({
        "device_id": "disk:legacy:zero",
        "name": "zero",
        "total_bytes": 0,
        "read_bytes_per_sec": 0,
        "write_bytes_per_sec": 0,
        "iops": 0,
        "active_time_pct": 0.0,
        "response_time_ms": 0.0
    }))
    .expect("decode legacy sentinel row");

    assert_eq!(decoded.current_capacity_bytes(), None);
    assert_eq!(decoded.current_read_bytes_per_sec(), None);
    assert_eq!(decoded.current_iops(), None);
    assert_eq!(decoded.current_active_time_pct(), None);
}

#[test]
fn typed_measured_zero_roundtrips_as_current_and_projects_legacy_zero() {
    let mut disk = DiskMetrics::new("measured-zero");
    disk.device_id = "disk:typed:zero".into();
    disk.apply_scalar_observations(DiskScalarObservations {
        capacity_bytes: ScalarObservation::available(0, 42),
        available_bytes: ScalarObservation::available(0, 42),
        read_bytes_per_sec: ScalarObservation::available(0, 42),
        write_bytes_per_sec: ScalarObservation::available(0, 42),
        iops: ScalarObservation::available(0, 42),
        active_time_pct: ScalarObservation::available(0.0, 42),
        response_time_ms: ScalarObservation::available(0.0, 42),
    });

    let wire = serde_json::to_value(&disk).expect("serialize typed zero");
    assert_eq!(wire["read_bytes_per_sec"], 0);
    assert_eq!(wire["active_time_pct"], 0.0);
    let decoded: DiskMetrics = serde_json::from_value(wire.clone()).expect("roundtrip typed zero");
    assert_eq!(decoded.current_capacity_bytes(), Some(0));
    assert_eq!(decoded.current_iops(), Some(0));
    assert_eq!(decoded.current_active_time_pct(), Some(0.0));

    let mut typed_only = wire;
    let object = typed_only.as_object_mut().expect("disk wire object");
    for key in [
        "total_bytes",
        "available_bytes",
        "read_bytes_per_sec",
        "write_bytes_per_sec",
        "iops",
        "active_time_pct",
        "response_time_ms",
    ] {
        object.remove(key);
    }
    let decoded: DiskMetrics =
        serde_json::from_value(typed_only).expect("typed-only disk remains readable");
    assert_eq!(decoded.current_capacity_bytes(), Some(0));
    assert_eq!(decoded.current_iops(), Some(0));
}

#[test]
fn typed_connection_and_attachment_facts_win_over_legacy_conflicts() {
    use crate::core::{StorageConnection, StorageDeviceKind, StorageInterconnect, StorageProtocol};

    let typed_connection = StorageConnection::new(
        StorageProtocol::Nvme,
        StorageInterconnect::Usb,
        StorageDeviceKind::Physical,
    );
    let decoded: DiskMetrics = serde_json::from_value(serde_json::json!({
        "device_id": "disk:conflict",
        "name": "conflict",
        "transport": "sata",
        "connection": typed_connection,
        "removable": true,
        "media_removable": false,
        "hotplug_capable": false
    }))
    .expect("decode disk conflict");

    assert_eq!(decoded.connection(), typed_connection);
    assert_eq!(decoded.media_removable(), Some(false));
    assert_eq!(decoded.hotplug_capable(), Some(false));
}
