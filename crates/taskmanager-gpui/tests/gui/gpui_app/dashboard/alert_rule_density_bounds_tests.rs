//! Compact alert-rule controls remain reachable without a horizontal clip.

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
async fn dense_alert_controls_fit_the_compact_dialog_viewport(cx: &mut TestAppContext) {
    pin_english();
    cx.update(|cx| {
        cx.text_system()
            .add_fonts(embedded_fonts())
            .expect("embedded fonts")
    });
    for (width, height) in [(640.0, 480.0), (720.0, 480.0), (1180.0, 780.0)] {
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
        for selector in [
            "alert-toggle-0",
            "alert-threshold-less-0",
            "alert-threshold-more-0",
            "alert-duration-less-0",
            "alert-duration-more-0",
            "alert-hysteresis-less-0",
            "alert-hysteresis-more-0",
        ] {
            let bounds = visual
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} must remain mounted at {width}x{height}"));
            assert!(
                bounds.left() >= viewport.left() && bounds.right() <= viewport.right(),
                "{selector} must stay inside the bounded viewport: {bounds:?} in {viewport:?}"
            );
        }
        let add = visual
            .debug_bounds("alert-add-rule")
            .expect("the bottom action remains mounted in the scroll group");
        assert!(
            add.left() >= viewport.left() && add.right() <= viewport.right(),
            "the bottom action must not clip horizontally: {add:?} in {viewport:?}"
        );
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
            .expect("the bottom action survives scrolling");
        assert!(
            scrolled_add.bottom() <= viewport.bottom(),
            "scrolling must reveal the complete bottom action: {add:?} -> {scrolled_add:?} in {viewport:?}"
        );
    }
}
