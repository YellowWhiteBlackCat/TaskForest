use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::core::storage_health::{FilesystemHealth, FilesystemHealthSnapshot};

use super::{aggregate_device_status, filesystem_health_sources};

fn fs(fs_type: &str, integrity: DeviceStatus) -> FilesystemHealth {
    FilesystemHealth {
        mount_point: "/mnt/test".into(),
        source: None,
        fs_type: fs_type.into(),
        read_only: None,
        error_count: None,
        status: taskmanager_core::core::storage_health::FilesystemHealthStatus::Healthy,
        state: DeviceState {
            status: DeviceStatus::Healthy,
            last_success_ms: None,
        },
        integrity_state: DeviceState {
            status: integrity,
            last_success_ms: None,
        },
    }
}

#[test]
fn empty_statuses_are_unsupported() {
    assert_eq!(
        aggregate_device_status([].into_iter()),
        DeviceStatus::Unsupported
    );
}

#[test]
fn permission_denied_outranks_everything() {
    for statuses in [
        vec![DeviceStatus::Healthy, DeviceStatus::PermissionDenied],
        vec![DeviceStatus::PermissionDenied, DeviceStatus::MissingTool],
        vec![
            DeviceStatus::Stale,
            DeviceStatus::PermissionDenied,
            DeviceStatus::MissingTool,
        ],
    ] {
        assert_eq!(
            aggregate_device_status(statuses.into_iter()),
            DeviceStatus::PermissionDenied,
            "permission denied is the most actionable state"
        );
    }
}

#[test]
fn priority_order_missing_tool_then_stale_then_healthy() {
    assert_eq!(
        aggregate_device_status([DeviceStatus::Healthy, DeviceStatus::MissingTool].into_iter()),
        DeviceStatus::MissingTool
    );
    assert_eq!(
        aggregate_device_status([DeviceStatus::Healthy, DeviceStatus::Stale].into_iter()),
        DeviceStatus::Stale
    );
    assert_eq!(
        aggregate_device_status([DeviceStatus::Unsupported, DeviceStatus::Healthy].into_iter()),
        DeviceStatus::Healthy
    );
}

#[test]
fn only_unsupported_statuses_aggregate_to_unsupported() {
    assert_eq!(
        aggregate_device_status([DeviceStatus::Unsupported, DeviceStatus::Unsupported].into_iter()),
        DeviceStatus::Unsupported
    );
}

#[test]
fn health_sources_cover_mountinfo_and_each_present_fs_type() {
    let snapshot = FilesystemHealthSnapshot {
        state: DeviceState {
            status: DeviceStatus::Healthy,
            last_success_ms: None,
        },
        filesystems: vec![
            fs("ext4", DeviceStatus::Healthy),
            fs("ext4", DeviceStatus::Healthy),
            fs("btrfs", DeviceStatus::Stale),
            fs("xfs", DeviceStatus::MissingTool),
        ],
    };
    let sources = filesystem_health_sources(&snapshot);
    assert_eq!(sources.len(), 4, "mountinfo + ext4 + btrfs + xfs");

    let mountinfo = &sources[0];
    assert_eq!(mountinfo.provider.as_str(), "linux.storage.mountinfo");
    assert_eq!(mountinfo.item_count, 4);

    let ext4 = sources
        .iter()
        .find(|s| s.provider.as_str() == "linux.storage.integrity.ext4")
        .expect("ext4 integrity source present");
    assert_eq!(ext4.item_count, 2, "healthy ext4 count");
    let btrfs = sources
        .iter()
        .find(|s| s.provider.as_str() == "linux.storage.integrity.btrfs")
        .expect("btrfs integrity source present");
    assert_eq!(btrfs.item_count, 0, "no healthy btrfs filesystem");
}

#[test]
fn health_sources_without_matching_fs_types_still_report_their_source() {
    let snapshot = FilesystemHealthSnapshot {
        state: DeviceState {
            status: DeviceStatus::Healthy,
            last_success_ms: None,
        },
        filesystems: vec![fs("vfat", DeviceStatus::Healthy)],
    };
    let sources = filesystem_health_sources(&snapshot);
    assert_eq!(
        sources.len(),
        4,
        "unknown fs types still yield per-type sources"
    );
    assert_eq!(
        sources[1].item_count, 0,
        "no ext4 filesystems means zero healthy ext4 items"
    );
}
