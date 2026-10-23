//! Behavior tests for the partition ↔ filesystem projection (CORE-55).

use crate::presentation::storage_health;
use std::path::PathBuf;
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::metrics::{DiskMetrics, DiskPartition};
use taskmanager_core::core::storage_health::{
    FilesystemBackingKind, FilesystemHealth, FilesystemHealthSnapshot, FilesystemHealthStatus,
};

fn partition(name: &str, mount_point: &str) -> DiskPartition {
    let mut partition = DiskPartition::new(name);
    partition.mount_point = mount_point.to_string();
    partition
}

fn disk_with_partitions(partitions: Vec<DiskPartition>) -> DiskMetrics {
    let mut disk = DiskMetrics::new("nvme0n1");
    disk.device_id = "disk:wwid:nvme0n1".to_string();
    disk.partitions = partitions;
    disk
}

fn filesystems(
    mount_point: &str,
    read_only: bool,
    error_count: Option<u64>,
) -> FilesystemHealthSnapshot {
    FilesystemHealthSnapshot {
        state: DeviceState::healthy(1_000),
        filesystems: vec![FilesystemHealth {
            mount_point: PathBuf::from(mount_point),
            source: Some(PathBuf::from("/dev/nvme0n1p2")),
            fs_type: "ext4".to_string(),
            backing_kind: FilesystemBackingKind::PhysicalBlock,
            read_only: Some(read_only),
            error_count,
            inode_used: None,
            inode_total: None,
            inode_usage_percent: None,
            status: if read_only {
                FilesystemHealthStatus::ReadOnly
            } else {
                FilesystemHealthStatus::Healthy
            },
            state: DeviceState::healthy(1_000),
            integrity_state: DeviceState::healthy(1_000),
        }],
    }
}

#[test]
fn a_mounted_partition_reads_its_filesystem_health_fact() {
    let disks = [disk_with_partitions(vec![
        partition("nvme0n1p1", "/"),
        partition("nvme0n1p2", "/data"),
    ])];
    let snapshot = filesystems("/data", true, Some(3));

    let facts = storage_health::partition_health_facts(&disks, Some(&snapshot));
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].disk_device_id, "disk:wwid:nvme0n1");
    assert!(
        facts[0].filesystem.is_none(),
        "a partition whose mount was not observed carries no filesystem fact"
    );
    let health = facts[1]
        .filesystem
        .expect("the mounted partition resolves its filesystem fact");
    assert_eq!(health.read_only, Some(true));
    assert_eq!(health.error_count, Some(3));
    assert_eq!(health.status, FilesystemHealthStatus::ReadOnly);
}

#[test]
fn an_unobserved_filesystem_snapshot_leaves_every_partition_without_a_fact() {
    let disks = [disk_with_partitions(vec![partition("nvme0n1p1", "/")])];
    let facts = storage_health::partition_health_facts(&disks, None);
    assert_eq!(facts.len(), 1);
    assert!(
        facts[0].filesystem.is_none(),
        "an unavailable filesystem provider must not fabricate a healthy row"
    );
}

#[test]
fn a_disk_without_partitions_projects_no_rows() {
    let disks = [disk_with_partitions(Vec::new())];
    let snapshot = filesystems("/", false, None);
    assert!(storage_health::partition_health_facts(&disks, Some(&snapshot)).is_empty());
}

#[test]
fn a_single_partition_resolves_its_filesystem_fact_directly() {
    let disks = [disk_with_partitions(vec![partition("nvme0n1p2", "/data")])];
    let snapshot = filesystems("/data", true, Some(7));
    let health =
        storage_health::partition_filesystem_health(&disks[0].partitions[0], Some(&snapshot))
            .expect("the mounted partition resolves its filesystem fact");
    assert_eq!(health.read_only, Some(true));
    assert_eq!(health.error_count, Some(7));
    assert!(
        storage_health::partition_filesystem_health(&disks[0].partitions[0], None).is_none(),
        "an unavailable filesystem provider resolves no fact"
    );
}
