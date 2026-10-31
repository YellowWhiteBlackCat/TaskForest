//! Behavior tests for the SMART remediation hint and the partition ↔
//! filesystem association (CORE-54, CORE-55).
//!
//! The shared fact owns the probe command and the guidance paragraph, and
//! the partition resolves its filesystem health fact inside the contract —
//! no consumer compares mount-point strings itself.

use super::*;
use crate::core::device_state::DeviceState;
use crate::core::storage_health::{
    FilesystemBackingKind, FilesystemHealth, FilesystemHealthSnapshot, FilesystemHealthStatus,
};
use std::path::PathBuf;

fn disk_with_smart(availability: SmartAvailability) -> DiskMetrics {
    let mut disk = DiskMetrics::new("nvme0n1");
    disk.smart_availability = availability;
    disk
}

#[test]
fn a_missing_smart_tool_carries_the_platform_neutral_remediation() {
    let disk = disk_with_smart(SmartAvailability::MissingTool);
    let remediation = disk
        .smart_tool_remediation_hint()
        .expect("a missing tool carries its remediation");
    assert_eq!(remediation.probe_command, "smartctl --scan");
    assert_eq!(remediation.guidance, SmartToolGuidance::InstallTool);
    assert_eq!(
        remediation.guidance.i18n_key(),
        "device.action_missing_tool"
    );
    assert_eq!(
        remediation.install_command, None,
        "the contract names no install command on its own"
    );
}

#[test]
fn a_present_smart_tool_carries_no_remediation() {
    for availability in [
        SmartAvailability::Available,
        SmartAvailability::Unsupported,
        SmartAvailability::Unavailable,
        SmartAvailability::PermissionDenied,
    ] {
        let disk = disk_with_smart(availability);
        assert!(
            disk.smart_tool_remediation_hint().is_none(),
            "availability {availability:?} must not claim a missing tool"
        );
    }
}

#[test]
fn the_platform_install_command_refines_the_platform_neutral_remediation() {
    let mut disk = disk_with_smart(SmartAvailability::MissingTool);
    disk.smart_tool_remediation = Some(
        SmartToolRemediation::platform_neutral()
            .with_install_command("sudo apt install smartmontools"),
    );
    let remediation = disk
        .smart_tool_remediation_hint()
        .expect("a missing tool carries its remediation");
    assert_eq!(remediation.probe_command, "smartctl --scan");
    assert_eq!(
        remediation.install_command.as_deref(),
        Some("sudo apt install smartmontools")
    );
}

#[test]
fn the_remediation_survives_the_disk_wire_boundary() {
    let mut disk = disk_with_smart(SmartAvailability::MissingTool);
    disk.smart_tool_remediation = Some(
        SmartToolRemediation::platform_neutral()
            .with_install_command("sudo apt install smartmontools"),
    );
    let serialized = serde_json::to_string(&disk).expect("serialize disk");
    let restored: DiskMetrics = serde_json::from_str(&serialized).expect("deserialize disk");
    assert_eq!(restored.smart_tool_remediation, disk.smart_tool_remediation);
    assert_eq!(
        restored
            .smart_tool_remediation_hint()
            .and_then(|remediation| remediation.install_command),
        Some("sudo apt install smartmontools".to_string())
    );
}

#[test]
fn a_partition_resolves_its_filesystem_health_fact() {
    let filesystem = FilesystemHealth {
        mount_point: PathBuf::from("/data"),
        source: Some(PathBuf::from("/dev/nvme0n1p2")),
        fs_type: "ext4".to_string(),
        backing_kind: FilesystemBackingKind::PhysicalBlock,
        read_only: Some(true),
        error_count: Some(3),
        inode_used: Some(100),
        inode_total: Some(1000),
        inode_usage_percent: Some(10.0),
        status: FilesystemHealthStatus::ReadOnly,
        state: DeviceState::healthy(1000),
        integrity_state: DeviceState::healthy(1000),
    };
    let filesystems = FilesystemHealthSnapshot {
        state: DeviceState::healthy(1000),
        filesystems: vec![filesystem],
    };

    let mut partition = DiskPartition::new("nvme0n1p2");
    partition.mount_point = "/data".to_string();
    let health = partition
        .filesystem_health(&filesystems)
        .expect("the mounted partition resolves its filesystem fact");
    assert_eq!(health.read_only, Some(true));
    assert_eq!(health.error_count, Some(3));
    assert_eq!(health.status, FilesystemHealthStatus::ReadOnly);
}

#[test]
fn an_unmounted_or_unmatched_partition_resolves_no_filesystem_fact() {
    let filesystems = FilesystemHealthSnapshot {
        state: DeviceState::healthy(1000),
        filesystems: vec![FilesystemHealth {
            mount_point: PathBuf::from("/data"),
            source: None,
            fs_type: "ext4".to_string(),
            backing_kind: FilesystemBackingKind::PhysicalBlock,
            read_only: Some(false),
            error_count: None,
            inode_used: None,
            inode_total: None,
            inode_usage_percent: None,
            status: FilesystemHealthStatus::Healthy,
            state: DeviceState::healthy(1000),
            integrity_state: DeviceState::healthy(1000),
        }],
    };

    let mut unmounted = DiskPartition::new("nvme0n1p3");
    unmounted.mount_point = String::new();
    assert!(
        unmounted.filesystem_health(&filesystems).is_none(),
        "an unmounted partition has no filesystem fact"
    );

    let mut other = DiskPartition::new("nvme0n1p4");
    other.mount_point = "/other".to_string();
    assert!(
        other.filesystem_health(&filesystems).is_none(),
        "a partition with no observed mount resolves no filesystem fact"
    );
}
