//! Page-family composition guard (ADR-041): every top-level page declares a
//! family, and each family owns exactly ONE composition root. The chart
//! surface must paint `tm-perf-main-viewport` (the `perf_page` root); every
//! data page must paint `tm-page-scaffold` (the shared `PageScaffold`
//! shell). A page that grows its own outer wrapper fails here before the
//! families can drift apart.

use gpui::{AppContext, TestAppContext, VisualTestContext, px, size};

use crate::gpui_app::root::{RootView, TopPage};
use crate::gpui_app::theme::Theme;

fn wrapped_root(cx: &mut TestAppContext) -> (gpui::WindowHandle<RootView>, gpui::Entity<RootView>) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    (win, view)
}

fn draw(cx: &mut TestAppContext, win: gpui::WindowHandle<RootView>) {
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

/// The typed family mapping is exhaustive and stable: exactly one chart
/// surface, every other page is a data surface.
#[test]
fn page_family_mapping_has_one_chart_surface() {
    use crate::gpui_app::root::navigation::PageFamily;
    assert_eq!(TopPage::Performance.family(), PageFamily::Chart);
    for page in TopPage::ALL {
        let expected = if page == TopPage::Performance {
            PageFamily::Chart
        } else {
            PageFamily::Data
        };
        assert_eq!(page.family(), expected, "{page:?} family drifted");
    }
}

/// Every DATA page composes through the one shared outer shell, and the
/// list-style inventory pages additionally share the one inner
/// header+body scaffold — so a skeleton adjustment in `PageScaffold` or
/// `ListPageScaffold` propagates to all of them at once.
#[gpui::test]
async fn every_data_page_paints_the_shared_page_scaffold(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    for page in [
        TopPage::Apps,
        TopPage::Services,
        TopPage::System,
        TopPage::Startup,
        TopPage::Users,
        TopPage::AppHistory,
        TopPage::Containers,
    ] {
        view.update(cx, |v, cx| {
            v.mark_telemetry_frame_ready();
            v.page = page;
            cx.notify();
        });
        draw(cx, win);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        if std::env::var("TM_FAMILY_PROBE").is_ok() {
            for probe in [
                "tm-page-scaffold",
                "tm-page-scaffold-footer",
                "tm-list-page-scaffold",
            ] {
                eprintln!(
                    "[family-probe] {page:?} {probe}={:?}",
                    vcx.debug_bounds(probe)
                );
            }
        }
        assert!(
            vcx.debug_bounds("tm-page-scaffold").is_some(),
            "{page:?} must compose through the shared data-page shell"
        );
    }
}

/// The CHART surface never mounts the data shell: it owns its own
/// composition root, and the two families stay structurally distinct.
#[gpui::test]
async fn chart_surface_uses_its_own_root_not_the_data_shell(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(1180.0), px(780.0)));
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Performance;
        cx.notify();
    });
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-perf-main-viewport").is_some(),
        "the Performance surface must compose through perf_page"
    );
    assert!(
        vcx.debug_bounds("tm-page-scaffold").is_none(),
        "the chart surface must not mount the data-page shell"
    );
}
