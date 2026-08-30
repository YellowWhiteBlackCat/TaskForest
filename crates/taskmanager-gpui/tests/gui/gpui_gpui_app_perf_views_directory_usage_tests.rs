use gpui::{AppContext, Modifiers, TestAppContext, VisualTestContext, px};

use crate::gpui_app::root::RootView;
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::metrics::{DiskMetrics, ScalarObservation};
use taskmanager_core::core::{
    DirectoryScanId, DirectoryScanStatus, DirectoryScanTotals, DirectoryUsageEntry,
    DirectoryUsageSnapshot, FailureKind,
};
use taskmanager_theme::Theme;

use super::*;

fn disk_with_mount(mount_point: &str) -> DiskMetrics {
    taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .partitions(vec![
            taskmanager_test_support::DiskPartitionFixtureBuilder::new()
                .mount_point(mount_point.to_string())
                .device_state(DeviceState::healthy(10))
                .build(),
        ])
        .build()
}

fn two_partition_disk() -> DiskMetrics {
    taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .partitions(vec![
            taskmanager_test_support::DiskPartitionFixtureBuilder::new()
                .mount_point("/".to_string())
                .build(),
            taskmanager_test_support::DiskPartitionFixtureBuilder::new()
                .mount_point("/home".to_string())
                .build(),
        ])
        .build()
}

fn snapshot(root: &str, status: DirectoryScanStatus) -> DirectoryUsageSnapshot {
    DirectoryUsageSnapshot {
        scan_id: DirectoryScanId::new(1),
        root: root.to_string(),
        status,
        entries: Vec::new(),
        totals: DirectoryScanTotals::fresh(10),
    }
}

fn entry(path: &str, depth: u32, size: Option<u64>) -> DirectoryUsageEntry {
    match size {
        Some(bytes) => DirectoryUsageEntry {
            path: path.to_string(),
            depth,
            size_bytes: ScalarObservation::available(bytes, 10),
            file_count: ScalarObservation::available(1, 10),
            unreadable: None,
        },
        None => DirectoryUsageEntry {
            path: path.to_string(),
            depth,
            size_bytes: ScalarObservation::unavailable(FailureKind::PermissionDenied),
            file_count: ScalarObservation::unavailable(FailureKind::PermissionDenied),
            unreadable: Some(FailureKind::PermissionDenied),
        },
    }
}

fn wrapped_root(cx: &mut TestAppContext) -> (gpui::WindowHandle<RootView>, gpui::Entity<RootView>) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    (win, view)
}

#[test]
fn snapshot_targeting_matches_own_partition_and_drilldown_paths_only() {
    let disk = disk_with_mount("/home");
    assert!(snapshot_targets_disk(
        &snapshot("/home", DirectoryScanStatus::Completed),
        &disk
    ));
    assert!(
        snapshot_targets_disk(
            &snapshot("/home/<user>/Downloads", DirectoryScanStatus::Completed),
            &disk
        ),
        "drill-down paths below the mount point belong to this disk"
    );
    assert!(
        !snapshot_targets_disk(&snapshot("/var", DirectoryScanStatus::Completed), &disk),
        "another mount tree must not match"
    );
    assert!(
        !snapshot_targets_disk(
            &snapshot("/homebrew", DirectoryScanStatus::Completed),
            &disk
        ),
        "a sibling with a shared prefix is not a child path"
    );
}

#[test]
fn root_mount_point_snapshot_matches_all_children() {
    let disk = disk_with_mount("/");
    assert!(snapshot_targets_disk(
        &snapshot("/", DirectoryScanStatus::Completed),
        &disk
    ));
    assert!(snapshot_targets_disk(
        &snapshot("/boot/efi", DirectoryScanStatus::Completed),
        &disk
    ));
}

#[test]
fn snapshot_without_mounted_partitions_never_matches() {
    let disk = disk_with_mount("");
    assert!(!snapshot_targets_disk(
        &snapshot("/", DirectoryScanStatus::Completed),
        &disk
    ));
}

#[test]
fn entry_with_unreadable_subtree_renders_the_typed_state_not_a_zero() {
    let unreadable = entry("secret", 1, None);
    assert!(unreadable.size_bytes.current_value().is_none());
    assert!(unreadable.unreadable.is_some());
    assert_eq!(
        unreadable.size_bytes.availability(),
        taskmanager_core::core::metrics::ScalarAvailability::Unavailable(
            FailureKind::PermissionDenied
        )
    );
}

struct TestUsageContainer {
    root: gpui::Entity<RootView>,
    disk: DiskMetrics,
}

impl gpui::Render for TestUsageContainer {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        let root = self.root.clone();
        let disk = self.disk.clone();
        root.update(cx, |v, cx| {
            directory_usage_panel(
                &v.theme,
                &disk,
                v.directory_usage(),
                UnitPreferences::default(),
                cx,
            )
        })
    }
}

fn wrapped_panel(
    cx: &mut TestAppContext,
    disk: DiskMetrics,
) -> (
    gpui::WindowHandle<TestUsageContainer>,
    gpui::Entity<RootView>,
) {
    let (_root_win, root_view) = wrapped_root(cx);
    let root_for_win = root_view.clone();
    let win = cx.add_window(|_window, _cx| TestUsageContainer {
        root: root_for_win,
        disk,
    });
    (win, root_view)
}

#[gpui::test]
async fn disk_page_paints_one_scan_pill_per_mounted_partition(cx: &mut TestAppContext) {
    let (win, _view) = wrapped_panel(cx, two_partition_disk());
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .expect("fixture draw");
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-disk-usage-panel").is_some(),
        "the usage panel must render on the disk page"
    );
    assert!(
        vcx.debug_bounds("tm-disk-usage-scan:0").is_some()
            && vcx.debug_bounds("tm-disk-usage-scan:1").is_some(),
        "each mounted partition gets one scan pill"
    );
    assert!(
        vcx.debug_bounds("tm-disk-usage-entry:0").is_none(),
        "no snapshot means no fabricated entry rows"
    );
}

#[gpui::test]
async fn disk_page_renders_bounded_report_rows_with_typed_unreadable_entries(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_panel(cx, disk_with_mount("/"));
    view.update(cx, |v, cx| {
        let mut snapshot = snapshot("/", DirectoryScanStatus::Completed);
        snapshot.entries = vec![
            entry("", 0, Some(500)),
            entry("big", 1, Some(400)),
            entry("secret", 1, None),
        ];
        v.replace_directory_usage_for_test(Some(snapshot));
        cx.notify();
    });
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .expect("fixture draw");
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    for selector in [
        "tm-disk-usage-entry:0",
        "tm-disk-usage-entry:1",
        "tm-disk-usage-entry:2",
    ] {
        let bounds = vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("entry row {selector} must render"));
        assert!(
            bounds.size.height > px(10.0),
            "row {selector} collapsed: {bounds:?}"
        );
    }
}

#[gpui::test]
async fn clicking_a_scan_pill_without_a_platform_worker_is_a_state_preserving_noop(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_panel(cx, two_partition_disk());
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .expect("fixture draw");
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let pill_bounds = vcx
        .debug_bounds("tm-disk-usage-scan:0")
        .expect("scan pill bounds");
    vcx.simulate_click(pill_bounds.center(), Modifiers::none());
    view.update(cx, |v, _cx| {
        assert!(
            v.directory_usage().is_none(),
            "without a platform worker the click must not fabricate a scan"
        );
    });
}
