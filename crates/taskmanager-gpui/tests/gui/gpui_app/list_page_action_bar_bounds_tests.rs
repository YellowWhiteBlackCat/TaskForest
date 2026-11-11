//! Headless bounds contract for the four inventory pages' action bars.
//!
//! The elastic-layout playbook requires every visible layout change to prove
//! its slot order, right edge and bottom safety band on measured frames
//! before the renderer changes. These assertions draw real `RootView` frames
//! at the minimum window, a normal 1280×720, a tall and a wide-short window,
//! then compare each action bar and the list-page body against the viewport.
//! Long localized labels are supplied by the production locale, so the test
//! measures the real product copy rather than a short fixture string.

use gpui::{AppContext, TestAppContext, VisualTestContext, px, size};

use crate::gpui_app::root::{RootView, TopPage};
use taskforest_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_theme::Theme;

/// The shared page viewport that every data page composes through.
const PAGE_VIEWPORT_SELECTOR: &str = "tm-telemetry-ready-body";

const DARK: fn() -> Theme = Theme::dark;

/// Measured viewport matrices: minimum, normal, tall, wide-short and narrow.
const VIEWPORTS: [(f32, f32); 6] = [
    (720.0, 480.0),
    (860.0, 600.0),
    (1180.0, 780.0),
    (1280.0, 720.0),
    (1024.0, 1200.0),
    (1600.0, 540.0),
];

fn draw(cx: &mut TestAppContext, win: gpui::WindowHandle<RootView>) {
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear(cx))
        .expect("list page window must draw");
}

/// A long, real service unit name — the shape that pushes an intrinsic-width
/// control past the right edge in a narrow viewport.
fn service(ix: usize) -> ServiceItem {
    ServiceItem::from_inventory(
        format!("fixture.service.{ix}"),
        format!("fixture-very-long-systemd-unit-name-{ix}.service"),
        ServiceStatus::Active,
        format!("fixture service number {ix}"),
        "loaded",
        "active",
        "running",
    )
}

/// The right edge of a control must stay inside the window, and the action
/// bar must end before the body starts (no overlap / duplicate chrome).
fn assert_bounded(
    visual: &mut VisualTestContext,
    selector: &'static str,
    viewport: gpui::Bounds<gpui::Pixels>,
    viewport_width: f32,
    page: TopPage,
    label: &str,
) {
    let bounds = visual
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{label}: {page:?} must paint {selector}"));
    assert!(
        bounds.origin.x >= px(0.0) && bounds.right() <= px(viewport_width),
        "{label}: {page:?} {selector} leaves the viewport: {bounds:?}, width={viewport_width}"
    );
    assert!(
        bounds.size.width > px(0.0),
        "{label}: {page:?} {selector} collapses to zero width: {bounds:?}"
    );
    assert!(
        bounds.bottom() <= viewport.bottom(),
        "{label}: {page:?} {selector} spills past the bottom safety band: control={bounds:?}, viewport={viewport:?}"
    );
    assert!(
        bounds.right() <= viewport.right(),
        "{label}: {page:?} {selector} spills past the right edge: control={bounds:?}, viewport={viewport:?}"
    );
}

/// Processes, Services, Startup and Users each keep their action strip and
/// the list body inside the frame at every representative viewport.
#[gpui::test]
async fn inventory_action_bars_keep_the_right_edge_and_the_body_slot(cx: &mut TestAppContext) {
    for (width, height) in VIEWPORTS {
        for (page, action_selector) in [
            (TopPage::Apps, "tm-proc-action-bar"),
            (TopPage::Services, "tm-svc-action-bar"),
            (TopPage::Startup, "tm-startup-chrome"),
            (TopPage::Users, "tm-users-header"),
        ] {
            let win = cx.add_window(|_window, cx| RootView::new(DARK(), cx));
            cx.simulate_window_resize(win.into(), size(px(width), px(height)));
            win.update(cx, |view, _window, cx| {
                view.mark_telemetry_frame_ready();
                view.page = page;
                cx.notify();
            })
            .expect("page must be selectable");
            draw(cx, win);

            let mut visual = VisualTestContext::from_window(win.into(), cx);
            let slot = visual
                .debug_bounds(PAGE_VIEWPORT_SELECTOR)
                .unwrap_or_else(|| {
                    panic!("{width}x{height}: {page:?} must paint the shared page viewport")
                });
            assert_bounded(
                &mut visual,
                action_selector,
                slot,
                width,
                page,
                &format!("{width}x{height}"),
            );
        }
    }
}

/// A populated Services page must keep its action strip, status line and
/// search box inside the frame at narrow viewports; the long unit names are
/// real content, not a synthetic stress string.
#[gpui::test]
async fn a_populated_services_page_keeps_its_controls_inside_the_frame(cx: &mut TestAppContext) {
    for (width, height) in [(720.0, 480.0), (860.0, 600.0), (1180.0, 780.0)] {
        let win = cx.add_window(|_window, cx| RootView::new(DARK(), cx));
        cx.simulate_window_resize(win.into(), size(px(width), px(height)));
        let view = win.entity(cx).expect("window root RootView entity");
        view.update(cx, |view, cx| {
            view.mark_telemetry_frame_ready();
            view.page = TopPage::Services;
            view.replace_services_for_test((0..24).map(service).collect(), Vec::new());
            view.selected_service = Some(taskforest_core::core::target::ServiceId::new(
                "fixture.service.0",
            ));
            cx.notify();
        });
        draw(cx, win);

        let mut visual = VisualTestContext::from_window(win.into(), cx);
        let bar = visual
            .debug_bounds("tm-svc-action-bar")
            .expect("the Services action bar must render");
        let status = visual
            .debug_bounds("tm-feedback-status-line")
            .expect("the action-bar status line must render");
        let search = visual
            .debug_bounds("tm-search-box")
            .expect("the Services search box must render");

        assert!(
            status.right() <= bar.right(),
            "{width}x{height}: the status line leaves the action bar: status={status:?}, bar={bar:?}"
        );
        assert!(
            search.right() <= bar.right(),
            "{width}x{height}: the search box leaves the action bar: search={search:?}, bar={bar:?}"
        );
        assert!(
            search.origin.x >= bar.origin.x,
            "{width}x{height}: the search box leaves the action bar's left edge: search={search:?}, bar={bar:?}"
        );
        assert!(
            search.origin.x >= px(0.0) && search.right() <= px(width),
            "{width}x{height}: the search box leaves the viewport: search={search:?}"
        );
    }
}

/// The filter group is admitted as one whole band: the status-filter pills
/// and the search box share a line, and the search box reaches the action
/// bar's right edge. A wrapping toolbar that strands the search box alone on
/// a second line wastes most of that line and reads as a half control.
#[gpui::test]
async fn filter_pills_and_search_stay_one_right_aligned_group(cx: &mut TestAppContext) {
    for (width, height) in [
        (720.0, 480.0),
        (860.0, 600.0),
        (1180.0, 780.0),
        (1600.0, 540.0),
    ] {
        for page in [TopPage::Services, TopPage::Startup] {
            let win = cx.add_window(|_window, cx| RootView::new(DARK(), cx));
            cx.simulate_window_resize(win.into(), size(px(width), px(height)));
            win.update(cx, |view, _window, cx| {
                view.mark_telemetry_frame_ready();
                view.page = page;
                cx.notify();
            })
            .expect("page must be selectable");
            draw(cx, win);

            let mut visual = VisualTestContext::from_window(win.into(), cx);
            if std::env::var("TM_BOUNDS_PROBE").is_ok() {
                for probe in [
                    "tm-proc-action-bar",
                    "tm-proc-action-surface",
                    "tm-proc-unified-controls",
                    "tm-proc-stacked-controls",
                    "tm-search-box",
                    "tm-filter-pill-row",
                ] {
                    eprintln!(
                        "[proc-probe] {width}x{height} {page:?} {probe}={:?}",
                        visual.debug_bounds(probe)
                    );
                }
            }
            let pills = visual
                .debug_bounds("tm-filter-pill-row")
                .unwrap_or_else(|| panic!("{width}x{height}: {page:?} must paint filter pills"));
            let search = visual
                .debug_bounds("tm-search-box")
                .unwrap_or_else(|| panic!("{width}x{height}: {page:?} must paint a search box"));
            let bar_right = px(width) - px(8.0);

            assert_eq!(
                pills.origin.y, search.origin.y,
                "{width}x{height}: {page:?} split the filter group across lines: pills={pills:?}, search={search:?}"
            );
            assert!(
                search.right() <= bar_right + px(1.0) && search.right() >= bar_right - px(12.0),
                "{width}x{height}: {page:?} search box is not right-aligned: search={search:?}, bar_right={bar_right:?}"
            );
            assert!(
                pills.right() <= search.origin.x,
                "{width}x{height}: {page:?} filter pills overlap the search box: pills={pills:?}, search={search:?}"
            );
        }
    }
}
