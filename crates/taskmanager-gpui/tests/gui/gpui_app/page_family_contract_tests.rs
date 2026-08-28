//! Page-family composition guard (ADR-042): every top-level page declares a
//! family, and each family owns exactly ONE composition root. The chart
//! surface must paint `tm-perf-main-viewport` (the `perf_page` root); every
//! data page must paint `tm-page-scaffold` (the shared `PageScaffold`
//! shell). A page that grows its own outer wrapper fails here before the
//! families can drift apart.
//!
//! All selector identities are imported constants shared with the producers
//! (`taskmanager_ui::layout::selectors`, `list_view`/`perf_views::layout`/
//! `root::render`): a selector cannot drift between the builder and this
//! assertion, and a typo is a compile error instead of a silent `None`.

use gpui::{AppContext, TestAppContext, VisualTestContext};

use crate::gpui_app::list_view::LIST_PAGE_SCAFFOLD_SELECTOR;
use crate::gpui_app::perf_views::PERF_MAIN_VIEWPORT_SELECTOR;
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::navigation::{PageFamily, TopPage};
use crate::gpui_app::root::render::TELEMETRY_READY_BODY_SELECTOR;
use crate::gpui_app::theme::Theme;
use taskmanager_ui::layout::selectors::{PAGE_SCAFFOLD, PAGE_SCAFFOLD_FOOTER};

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

/// Every top-level page paints ITS family's composition root, proven on a
/// FRESH window per page: one page's deferred render work (service list
/// internals, focus/scroll handles) can otherwise leak into the next manual
/// draw in the harness and make a later page observe the previous page's
/// frame. A fresh surface proves each page composes correctly on its own,
/// which is what the product contract demands.
///
/// The loop is exhaustive over `TopPage::ALL`, so a NEW page is covered the
/// moment it exists. Per-page expectations come from the typed declarations
/// (`family()`, `uses_list_scaffold()`), never from a second list mirrored
/// here — the guard reads the mapping instead of re-stating it.
#[gpui::test]
async fn every_top_level_page_paints_its_family_root(cx: &mut TestAppContext) {
    for page in TopPage::ALL {
        let (win, view) = wrapped_root(cx);
        view.update(cx, |v, cx| {
            v.mark_telemetry_frame_ready();
            v.page = page;
            cx.notify();
        });
        draw(cx, win);
        let mut vcx = VisualTestContext::from_window(win.into(), cx);
        if std::env::var("TM_FAMILY_PROBE").is_ok() {
            for probe in [
                PAGE_SCAFFOLD,
                PAGE_SCAFFOLD_FOOTER,
                LIST_PAGE_SCAFFOLD_SELECTOR,
                TELEMETRY_READY_BODY_SELECTOR,
            ] {
                eprintln!(
                    "[family-probe] {page:?} {probe}={:?}",
                    vcx.debug_bounds(probe)
                );
            }
        }

        match page.family() {
            PageFamily::Chart => {
                assert!(
                    vcx.debug_bounds(PERF_MAIN_VIEWPORT_SELECTOR).is_some(),
                    "{page:?} must compose through its own chart root ({PERF_MAIN_VIEWPORT_SELECTOR} is missing from the rendered frame)"
                );
                assert!(
                    vcx.debug_bounds(PAGE_SCAFFOLD).is_none(),
                    "the chart surface must not mount the data-page shell ({PAGE_SCAFFOLD} was found in the frame)"
                );
                assert!(
                    vcx.debug_bounds(LIST_PAGE_SCAFFOLD_SELECTOR).is_none(),
                    "the chart surface must not mount the list-page shell"
                );
            }
            PageFamily::Data => {
                assert!(
                    vcx.debug_bounds(PAGE_SCAFFOLD).is_some(),
                    "{page:?} must compose through the shared data-page shell ({PAGE_SCAFFOLD} is missing from the rendered frame)"
                );
                let list_painted = vcx.debug_bounds(LIST_PAGE_SCAFFOLD_SELECTOR).is_some();
                assert_eq!(
                    list_painted,
                    page.uses_list_scaffold(),
                    "{page:?} list-shell presence drifted: uses_list_scaffold() = {}, {LIST_PAGE_SCAFFOLD_SELECTOR} painted = {list_painted}",
                    page.uses_list_scaffold()
                );
            }
        }
    }
}
