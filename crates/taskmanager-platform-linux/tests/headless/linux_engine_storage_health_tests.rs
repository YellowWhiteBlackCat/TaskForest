use super::*;

const MOUNTINFO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/proc_mountinfo.txt"
));

#[test]
fn mountinfo_preserves_read_only_and_unknown_integrity() {
    let filesystems = parse_mountinfo(MOUNTINFO, 100);
    assert_eq!(filesystems.len(), 3);
    assert_eq!(filesystems[0].mount_point, Path::new("/"));
    assert_eq!(filesystems[0].read_only, Some(false));
    assert_eq!(filesystems[1].status, FilesystemHealthStatus::ReadOnly);
    assert_eq!(filesystems[2].mount_point, Path::new("/media/My Disk"));
    assert_eq!(filesystems[2].error_count, None);
    assert_eq!(
        filesystems[2].integrity_state.status,
        DeviceStatus::Unsupported
    );
}

#[cfg(target_os = "linux")]
#[test]
fn ext4_error_counter_is_typed_and_missing_counter_stays_none() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-storage-health-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("sys/ext4/nvme0n1p2")).unwrap();
    std::fs::write(root.join("mountinfo"), MOUNTINFO).unwrap();
    std::fs::write(root.join("sys/ext4/nvme0n1p2/errors_count"), "3\n").unwrap();
    let snapshot = collect_filesystem_health_from(&root.join("mountinfo"), &root.join("sys"), 500);
    assert_eq!(snapshot.filesystems[0].error_count, Some(3));
    assert_eq!(
        snapshot.filesystems[0].status,
        FilesystemHealthStatus::ErrorsReported
    );
    assert_eq!(snapshot.filesystems[2].error_count, None);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn btrfs_and_xfs_health_parsers_preserve_zero_and_reported_errors() {
    assert_eq!(
        parse_btrfs_error_stats(
            "write_errs 0\nread_errs 2\nflush_errs 0\ncorruption_errs 1\ngeneration_errs 0\n"
        ),
        Some(3)
    );
    assert_eq!(parse_btrfs_error_stats("not-a-counter 9\n"), None);
    assert_eq!(
        parse_xfs_health_output("Health Status: 1/1 checked, 0 warnings\nfilesystem: healthy\n"),
        Some(0)
    );
    assert_eq!(
        parse_xfs_health_output("AG 0 metadata is sick\n2 warnings\n"),
        Some(3)
    );
    assert_eq!(
        parse_xfs_health_output("unrecognised provider output"),
        None
    );
}

#[cfg(target_os = "linux")]
#[test]
fn btrfs_sysfs_error_stats_are_aggregated_without_a_privileged_command() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-btrfs-health-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let fs = root.join("sys/btrfs/fixture-fsid");
    std::fs::create_dir_all(fs.join("devices/nvme0n1p2")).unwrap();
    std::fs::create_dir_all(fs.join("devinfo/1")).unwrap();
    std::fs::write(
        fs.join("devinfo/1/error_stats"),
        "write_errs 1\nread_errs 2\nflush_errs 0\ncorruption_errs 0\ngeneration_errs 0\n",
    )
    .unwrap();
    std::fs::write(
        root.join("mountinfo"),
        "36 25 259:2 / / rw,relatime - btrfs /dev/nvme0n1p2 rw\n",
    )
    .unwrap();
    let snapshot = collect_filesystem_health_from(&root.join("mountinfo"), &root.join("sys"), 700);
    assert_eq!(snapshot.filesystems[0].error_count, Some(3));
    assert_eq!(
        snapshot.filesystems[0].status,
        FilesystemHealthStatus::ErrorsReported
    );
    assert_eq!(
        snapshot.filesystems[0].integrity_state,
        DeviceState::healthy(700)
    );
    std::fs::remove_dir_all(root).ok();
}
