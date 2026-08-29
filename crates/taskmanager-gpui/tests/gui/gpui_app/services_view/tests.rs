//! Headless render-geometry regression tests for the Services page.
//!
//! Guard for "data arrived but the UI shows nothing": with materialized Services
//! populated, the virtualized Table must paint real rows with sane geometry —
//! and the empty-state branch must render when no services exist. The delta
//! between the two phases proves the table follows the data.

use gpui::{AppContext, TestAppContext, VisualTestContext, px};

use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use crate::gpui_app::root::{RootView, TopPage};
use taskmanager_theme::Theme;

fn service(ix: usize, status: ServiceStatus) -> ServiceItem {
    ServiceItem::from_inventory(
        format!("fixture.service.{ix}"),
        format!("fixture-service-{ix}"),
        status,
        format!("fixture service number {ix}"),
        "loaded",
        status.as_str().to_ascii_lowercase(),
        "running",
    )
}

fn wrapped_root(cx: &mut TestAppContext) -> (gpui::WindowHandle<RootView>, gpui::Entity<RootView>) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    (win, view)
}

fn draw(cx: &mut TestAppContext, win: gpui::WindowHandle<RootView>) {
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

/// Render-path assertion (后置): when no services exist the page renders its
/// empty-state branch (no table rows); once the materialized snapshot carries data,
/// the virtualized Table must paint one row per service with sane geometry and
/// rows stacked vertically. Catches "data arrived but the Services list stays
/// empty" regressions (e.g. a wrapper element dropping rows from the
/// uniform_list).
#[gpui::test]
async fn services_page_paints_rows_for_each_provided_service(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Services;
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-svc-row:0").is_none(),
        "the empty-state branch must not fabricate a table row"
    );

    view.update(cx, |v, cx| {
        v.replace_services_for_test(
            vec![
                service(0, ServiceStatus::Active),
                service(1, ServiceStatus::Inactive),
                service(2, ServiceStatus::Failed),
            ],
            Vec::new(),
        );
        cx.notify();
    });
    vcx.update(|window, cx| window.draw(cx).clear());
    for sel in ["tm-svc-row:0", "tm-svc-row:1", "tm-svc-row:2"] {
        let r = vcx
            .debug_bounds(sel)
            .unwrap_or_else(|| panic!("{sel} must render when services exist"));
        assert!(r.size.height > px(10.0), "{sel} collapsed: {r:?}");
        assert!(r.size.width > px(0.0), "{sel} has no width: {r:?}");
    }
    let r0 = vcx.debug_bounds("tm-svc-row:0").expect("row 0");
    let r2 = vcx.debug_bounds("tm-svc-row:2").expect("row 2");
    assert!(
        r2.origin.y > r0.origin.y,
        "service rows must stack vertically: {r0:?} vs {r2:?}"
    );
}
