use gpui::TestAppContext;

use crate::core::DirectoryScanTotals;
use crate::gpui_app::theme::Theme;

use super::*;

fn snapshot(scan_id: u64, status: DirectoryScanStatus) -> DirectoryUsageSnapshot {
    DirectoryUsageSnapshot {
        scan_id: DirectoryScanId::new(scan_id),
        root: "/fixture".to_string(),
        status,
        entries: Vec::new(),
        totals: DirectoryScanTotals::fresh(10),
    }
}

#[test]
fn active_scan_id_is_present_only_while_scanning() {
    assert_eq!(
        active_scan_id(Some(&snapshot(7, DirectoryScanStatus::Scanning))),
        Some(DirectoryScanId::new(7))
    );
    assert_eq!(
        active_scan_id(Some(&snapshot(7, DirectoryScanStatus::Completed))),
        None,
        "a terminal result stays visible but is not cancellable"
    );
    assert_eq!(active_scan_id(None), None);
}

#[gpui::test]
async fn scan_submissions_without_a_platform_worker_fall_back_to_honest_noops(
    cx: &mut TestAppContext,
) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |v, _win, cx| {
        assert!(
            !v.start_directory_scan("/home".to_string()),
            "no platform worker means no scan is submitted"
        );
        assert!(v.directory_usage().is_none());
        v.replace_directory_usage_for_test(Some(snapshot(1, DirectoryScanStatus::Scanning)));
        assert!(
            !v.cancel_directory_scan(),
            "an active scan with no platform worker still cannot submit"
        );
        assert_eq!(
            v.directory_usage().map(|s| s.scan_id),
            Some(DirectoryScanId::new(1)),
            "the failed submission must not mutate the stored snapshot"
        );
        v.replace_directory_usage_for_test(Some(snapshot(1, DirectoryScanStatus::Completed)));
        assert!(
            !v.cancel_directory_scan(),
            "a terminal scan is not cancellable even with a platform"
        );
        cx.notify();
    })
    .expect("fixture window update");
}
