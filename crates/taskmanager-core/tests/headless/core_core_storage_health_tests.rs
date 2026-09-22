use std::path::PathBuf;

use super::*;
use crate::core::device_state::DeviceState;

#[test]
fn storage_health_threshold_constants_are_finite_and_ordered() {
    const {
        assert!(DEFAULT_INODE_USAGE_WARNING_PERCENT > 0.0);
        assert!(DEFAULT_INODE_USAGE_CRITICAL_PERCENT > DEFAULT_INODE_USAGE_WARNING_PERCENT);
        assert!(DEFAULT_INODE_USAGE_CRITICAL_PERCENT <= 100.0);
        assert!(DEFAULT_NVME_WEAR_WARNING_PERCENT > 0.0);
        assert!(DEFAULT_NVME_WEAR_CRITICAL_PERCENT > DEFAULT_NVME_WEAR_WARNING_PERCENT);
        assert!(DEFAULT_NVME_SPARE_WARNING_PERCENT > DEFAULT_NVME_SPARE_CRITICAL_PERCENT);
        assert!(DEFAULT_NVME_SPARE_CRITICAL_PERCENT >= 0.0);
        assert!(DEFAULT_STORAGE_ALERT_HYSTERESIS_PERCENT > 0.0);
        assert!(DEFAULT_STORAGE_ALERT_HYSTERESIS_PERCENT < DEFAULT_INODE_USAGE_WARNING_PERCENT);
    }
    assert_eq!(DEFAULT_NVME_WEAR_CRITICAL_PERCENT, 100.0);
}

#[test]
fn nvme_spare_threshold_evaluation_is_remaining_capacity_based() {
    assert!(!is_nvme_spare_warning(10.1));
    assert!(is_nvme_spare_warning(10.0));
    assert!(!is_nvme_spare_critical(10.0));
    assert!(is_nvme_spare_critical(5.0));
    assert!(is_nvme_spare_critical(0.0));
}

#[test]
fn inode_usage_percent_computes_correctly() {
    assert_eq!(inode_usage_percent(0, 1000), Some(0.0));
    assert_eq!(inode_usage_percent(500, 1000), Some(50.0));
    assert_eq!(inode_usage_percent(850, 1000), Some(85.0));
    assert_eq!(inode_usage_percent(950, 1000), Some(95.0));
    assert_eq!(inode_usage_percent(1000, 1000), Some(100.0));

    // Zero denominator returns None to avoid divide-by-zero
    assert_eq!(inode_usage_percent(0, 0), None);
    assert_eq!(inode_usage_percent(100, 0), None);
}

#[test]
fn nvme_wear_threshold_evaluation_matches_spec() {
    // Under warning
    assert!(!is_nvme_wear_warning(89.9));
    assert!(!is_nvme_wear_critical(89.9));

    // At and above warning
    assert!(is_nvme_wear_warning(90.0));
    assert!(!is_nvme_wear_critical(90.0));
    assert!(is_nvme_wear_warning(95.0));
    assert!(!is_nvme_wear_critical(95.0));

    // At and above critical (endurance limit exhausted per NVMe spec)
    assert!(is_nvme_wear_warning(100.0));
    assert!(is_nvme_wear_critical(100.0));
    assert!(is_nvme_wear_warning(105.0));
    assert!(is_nvme_wear_critical(105.0));
}

#[test]
fn filesystem_health_snapshot_serde_roundtrip() {
    let health = FilesystemHealth {
        mount_point: PathBuf::from("/"),
        source: Some(PathBuf::from("/dev/nvme0n1p2")),
        fs_type: "ext4".to_string(),
        backing_kind: FilesystemBackingKind::PhysicalBlock,
        read_only: Some(false),
        error_count: Some(0),
        inode_used: Some(100),
        inode_total: Some(1000),
        inode_usage_percent: Some(10.0),
        status: FilesystemHealthStatus::Healthy,
        state: DeviceState::healthy(1000),
        integrity_state: DeviceState::healthy(1000),
    };
    let snapshot = FilesystemHealthSnapshot {
        state: DeviceState::healthy(1000),
        filesystems: vec![health.clone()],
    };

    let serialized = serde_json::to_string(&snapshot).expect("serialize snapshot");
    let deserialized: FilesystemHealthSnapshot =
        serde_json::from_str(&serialized).expect("deserialize snapshot");

    assert_eq!(deserialized, snapshot);
    assert_eq!(
        deserialized.filesystems[0].status,
        FilesystemHealthStatus::Healthy
    );
}
