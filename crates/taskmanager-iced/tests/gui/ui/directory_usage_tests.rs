use super::*;
use taskmanager_core::core::directory_usage::{DirectoryScanId, DirectoryScanTotals};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::ScalarObservation;

/// A disk whose partition children carry `mount_points`, in order. The
/// partition type is not re-exported to frontends, so the children are
/// built through `Default` with the element type inferred from the
/// field itself.
fn disk_with_mounts(mount_points: &[&str]) -> DiskMetrics {
    taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .partitions({
            let mut children = DiskMetrics::default().partitions;
            children.resize(mount_points.len(), Default::default());
            for (child, mount_point) in children.iter_mut().zip(mount_points) {
                child.mount_point = (*mount_point).to_string();
            }
            children
        })
        .build()
}

fn disk_with_mount(mount_point: &str) -> DiskMetrics {
    disk_with_mounts(&[mount_point])
}

fn snapshot(root: &str, status: DirectoryScanStatus) -> DirectoryUsageSnapshot {
    DirectoryUsageSnapshot {
        scan_id: DirectoryScanId::new(7),
        root: root.to_string(),
        status,
        entries: Vec::new(),
        totals: DirectoryScanTotals::fresh(10),
    }
}

fn entry(path: &str, depth: u32, size: Option<u64>, unreadable: bool) -> DirectoryUsageEntry {
    let failure = if unreadable {
        Some(FailureKind::PermissionDenied)
    } else {
        None
    };
    let size_bytes = match size {
        Some(bytes) => ScalarObservation::available(bytes, 10),
        None => ScalarObservation::unavailable(FailureKind::PermissionDenied),
    };
    DirectoryUsageEntry {
        path: path.to_string(),
        depth,
        size_bytes,
        file_count: ScalarObservation::available(1, 10),
        unreadable: failure,
    }
}

#[test]
fn idle_disk_starts_a_scan_of_its_first_mounted_partition() {
    let disk = disk_with_mounts(&["", "/data"]);
    let request = toggle_request(&disk, None).expect("an action for a mounted disk");
    match request {
        DirectoryUsageRequest::StartScan(spec) => {
            assert_eq!(spec.root, "/data", "the first MOUNTED partition wins");
            assert_eq!(spec.bounds, DirectoryScanBounds::default());
        }
        other => panic!("expected a start request, got {other:?}"),
    }
}

#[test]
fn disk_without_reported_mounts_falls_back_to_the_root_path() {
    let disk = disk_with_mount("");
    let request = toggle_request(&disk, None).expect("a fallback action");
    match request {
        DirectoryUsageRequest::StartScan(spec) => assert_eq!(spec.root, "/"),
        other => panic!("expected a start request, got {other:?}"),
    }
}

/// The disk-level mount point (the demo / whole-disk-filesystem shape,
/// no partition children) owns its own tree: it is the scan root and it
/// matches snapshots rooted at or below it.
#[test]
fn a_disk_level_mount_without_partition_children_owns_its_tree() {
    let disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .mount_point("/".into())
        .build();
    assert!(disk.partitions.is_empty());
    assert!(has_reported_mount(&disk));
    match toggle_request(&disk, None).expect("an action for a disk-level mount") {
        DirectoryUsageRequest::StartScan(spec) => assert_eq!(spec.root, "/"),
        other => panic!("expected a start request, got {other:?}"),
    }
    assert!(snapshot_targets_disk(
        &snapshot("/", DirectoryScanStatus::Scanning),
        &disk
    ));
    assert!(snapshot_targets_disk(
        &snapshot("/var/log", DirectoryScanStatus::Scanning),
        &disk
    ));
}

#[test]
fn an_active_scan_of_this_disk_toggles_to_cancel_by_scan_id() {
    let disk = disk_with_mount("/home");
    let active = snapshot("/home", DirectoryScanStatus::Scanning);
    assert_eq!(
        toggle_request(&disk, Some(&active)),
        Some(DirectoryUsageRequest::Cancel(DirectoryScanId::new(7)))
    );
    // A terminal scan re-starts instead of cancelling a finished id.
    for status in [
        DirectoryScanStatus::Completed,
        DirectoryScanStatus::Cancelled,
        DirectoryScanStatus::Failed(FailureKind::PermissionDenied),
    ] {
        let terminal = snapshot("/home", status);
        assert!(
            matches!(
                toggle_request(&disk, Some(&terminal)),
                Some(DirectoryUsageRequest::StartScan(_))
            ),
            "a terminal slot restarts: {status:?}"
        );
    }
    // Another disk's active scan is NOT this disk's cancel.
    let foreign = snapshot("/var", DirectoryScanStatus::Scanning);
    assert!(matches!(
        toggle_request(&disk, Some(&foreign)),
        Some(DirectoryUsageRequest::StartScan(_))
    ));
}

#[test]
fn snapshot_targeting_matches_own_partition_and_drilldown_paths_only() {
    let disk = disk_with_mount("/home");
    assert!(snapshot_targets_disk(
        &snapshot("/home", DirectoryScanStatus::Completed),
        &disk
    ));
    assert!(snapshot_targets_disk(
        &snapshot("/home/<user>/Downloads", DirectoryScanStatus::Completed),
        &disk
    ));
    assert!(!snapshot_targets_disk(
        &snapshot("/var", DirectoryScanStatus::Completed),
        &disk
    ));
    assert!(!snapshot_targets_disk(
        &snapshot("/homebrew", DirectoryScanStatus::Completed),
        &disk
    ));
    // The root partition covers every path below it.
    let root_disk = disk_with_mount("/");
    assert!(snapshot_targets_disk(
        &snapshot("/boot/efi", DirectoryScanStatus::Completed),
        &root_disk
    ));
}

/// Every typed status renders its own label — including each terminal
/// state — and the panel seams never fabricate values.
#[test]
fn every_typed_scan_status_renders_its_own_label() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    assert_eq!(
        status_text(DirectoryScanStatus::Scanning),
        "Scanning…",
        "the shared catalog resolves under the test language"
    );
    assert_eq!(status_text(DirectoryScanStatus::Completed), "Scan complete");
    assert_eq!(
        status_text(DirectoryScanStatus::Cancelled),
        "Scan cancelled"
    );
    assert_eq!(
        status_text(DirectoryScanStatus::Failed(FailureKind::TimedOut)),
        "Scan failed"
    );
}

#[test]
fn unreadable_entries_render_the_danger_label_not_a_fabricated_zero() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let unreadable = entry("secret", 1, None, true);
    assert!(entry_is_unreadable(&unreadable));
    assert_eq!(entry_size_text(&unreadable), "unreadable");

    let measured_zero = entry("empty", 1, Some(0), false);
    assert!(!entry_is_unreadable(&measured_zero));
    assert_eq!(
        entry_size_text(&measured_zero),
        "0 B",
        "measured zero stays a real value"
    );

    let absent = entry("gone", 1, None, false);
    assert!(!entry_is_unreadable(&absent));
    assert_eq!(entry_size_text(&absent), "—");
}

#[test]
fn totals_line_carries_each_failure_dimension_as_its_own_fact() {
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut snap = snapshot("/", DirectoryScanStatus::Completed);
    snap.totals.files_counted = 12;
    snap.totals.directories_visited = 3;
    snap.totals.bytes_counted =
        ScalarObservation::partial(4_096, 10, FailureKind::PermissionDenied);
    snap.totals.unreadable_directories = 2;
    snap.totals.capped = true;
    let line = totals_text(&snap);
    assert!(line.contains("12"), "file count: {line}");
    assert!(line.contains("4.0 KiB"), "typed byte sum: {line}");
    assert!(line.contains("partial"), "partial mark: {line}");
    assert!(line.contains("2 unreadable"), "unreadable count: {line}");
    assert!(line.contains("scan limits reached"), "capped bound: {line}");

    let clean = snapshot("/", DirectoryScanStatus::Completed);
    let clean_line = totals_text(&clean);
    assert!(!clean_line.contains("partial"));
    assert!(!clean_line.contains("scan limits reached"));
}

#[test]
fn the_report_collapses_beyond_the_presentational_cap() {
    assert_eq!(visible_entries(3), 3);
    assert_eq!(visible_entries(50), MAX_VISIBLE_ENTRIES);
}

/// The panel renders each typed state end-to-end from a fixture shell —
/// the element tree constructs for the idle, scanning, and every
/// terminal status without a live platform.
#[test]
fn panel_renders_each_typed_state_from_a_fixture_shell() {
    let mut app = crate::IcedApp::demo();
    let disk = disk_with_mount("/");
    // The rendered element borrows the shell; scope it out before the
    // fixture mutations below.
    {
        let _idle = usage_panel(&app, &disk);
    }

    let statuses = [
        DirectoryScanStatus::Scanning,
        DirectoryScanStatus::Completed,
        DirectoryScanStatus::Cancelled,
        DirectoryScanStatus::Failed(FailureKind::PermissionDenied),
    ];
    for status in statuses {
        let mut snap = snapshot("/", status);
        snap.entries = vec![
            entry("", 0, Some(500), false),
            entry("big", 1, Some(400), false),
            entry("secret", 1, None, true),
        ];
        taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::DirectoryUsage(Some(snap)),
        );
        {
            let _panel = usage_panel(&app, &disk);
        }
    }
    // A disk with no reported mounts renders the no-mounts hint branch.
    let bare = disk_with_mount("");
    {
        let _no_mounts = usage_panel(&app, &bare);
    }
}
