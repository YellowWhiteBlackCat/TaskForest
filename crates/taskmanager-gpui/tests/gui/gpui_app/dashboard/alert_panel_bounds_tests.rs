//! Alert-rule dialog bounds across standard and compact page sizes.

use crate::gpui_app::dashboard::DashboardPanel;
use crate::gpui_app::root::RootView;
use gpui::{
    AppContext, Modifiers, ScrollDelta, ScrollEvent, ScrollSource, TestAppContext, TouchPhase,
    VisualTestContext, point, px, size,
};
use taskmanager_assets::embedded_fonts;
use taskmanager_test_support::pin_english;
use taskmanager_theme::Theme;

#[gpui::test]
async fn alert_rule_controls_stay_inside_the_bounded_dialog_at_standard_and_compact_sizes(
    cx: &mut TestAppContext,
) {
    pin_english();
    cx.update(|cx| {
        cx.text_system()
            .add_fonts(embedded_fonts())
            .expect("embedded fonts")
    });
    for (width, height) in [(720.0, 480.0), (1180.0, 780.0)] {
        let window = cx.add_window(|_, cx| RootView::new_demo(Theme::dark(), cx));
        cx.simulate_window_resize(window.into(), size(px(width), px(height)));
        window
            .update(cx, |view, _, cx| {
                view.show_dashboard_panel(DashboardPanel::AlertRules);
                cx.notify();
            })
            .unwrap();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let viewport = visual
            .debug_bounds("tm-dashboard-panel-scroll")
            .expect("bounded dialog viewport");
        assert!(viewport.size.width > px(0.0) && viewport.size.height > px(0.0));
        assert!(viewport.right() <= px(width) && viewport.bottom() <= px(height));
        for selector in [
            "alert-toggle-0",
            "alert-threshold-less-0",
            "alert-duration-more-0",
            "alert-hysteresis-less-0",
            "alert-add-rule",
            "tm-dashboard-panel-scrollbar",
            "tm-dashboard-panel-scrollbar-track",
        ] {
            let bounds = visual
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} must remain reachable at {width}x{height}"));
            assert!(
                bounds.left() >= px(0.0) && bounds.right() <= px(width),
                "{selector} right edge: {bounds:?}"
            );
        }
        let add = visual
            .debug_bounds("alert-add-rule")
            .expect("bottom action must be mounted in the scroll group");
        let rail = visual
            .debug_bounds("tm-dashboard-panel-scrollbar")
            .expect("pinned rail");
        assert_eq!(rail.origin.y, viewport.origin.y);
        assert_eq!(rail.bottom(), viewport.bottom());
        assert!(viewport.bottom() <= px(height));
        for _ in 0..8 {
            visual.simulate_event(ScrollEvent {
                position: viewport.center(),
                delta: ScrollDelta::Pixels(point(px(0.0), px(-240.0))),
                modifiers: Modifiers::none(),
                touch_phase: TouchPhase::Moved,
                source: ScrollSource::Pointer,
            });
        }
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let scrolled_add = visual
            .debug_bounds("alert-add-rule")
            .expect("bottom action must survive scrolling");
        assert!(
            scrolled_add.bottom() <= viewport.bottom(),
            "scrolling must reveal the complete bottom action inside its bounded viewport: \
             {add:?} -> {scrolled_add:?} in {viewport:?}"
        );
    }
}
