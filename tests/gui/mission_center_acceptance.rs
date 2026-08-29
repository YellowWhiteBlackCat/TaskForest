//! End-to-end headless smoke paths for the Mission Center parity surface.

use gpui::{AppContext, Keystroke, TestAppContext, VisualTestContext, WindowHandle};
use taskmanager_gpui::gpui_app::dashboard::SystemSection;
use taskmanager_gpui::gpui_app::root::{RootView, TopPage};
use taskmanager_theme::Theme;

fn root(cx: &mut TestAppContext) -> WindowHandle<RootView> {
    cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx))
}

fn draw(cx: &mut TestAppContext, window: WindowHandle<RootView>) {
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
}

#[gpui::test]
async fn mc00_nav_complete_pointer_case_all_top_level_tabs_are_pointer_reachable_and_settings_escapes(
    cx: &mut TestAppContext,
) {
    let window = root(cx);
    let view = window.entity(cx).expect("RootView window entity");
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        cx.notify();
    });
    draw(cx, window);

    for (selector, expected) in [
        ("Performance", TopPage::Performance),
        ("Apps", TopPage::Apps),
        ("Services", TopPage::Services),
        ("System", TopPage::System),
        ("Startup", TopPage::Startup),
        ("Users", TopPage::Users),
    ] {
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        let bounds = visual
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("top-level tab {selector} must render"));
        visual.simulate_click(bounds.center(), Default::default());
        assert_eq!(
            view.read_with(cx, |view, _cx| view.page),
            expected,
            "pointer activation of {selector} must reach RootView::page"
        );
        drop(visual);
        draw(cx, window);
    }

    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let settings = visual
        .debug_bounds("settings-btn")
        .expect("settings control must remain reachable after page navigation");
    visual.simulate_click(settings.center(), Default::default());
    drop(visual);
    assert!(
        view.read_with(cx, |view, _cx| view.settings_open()),
        "pointer activation must open Settings"
    );

    cx.dispatch_keystroke(window.into(), Keystroke::parse("escape").unwrap());
    assert!(
        !view.read_with(cx, |view, _cx| view.settings_open()),
        "Escape must cancel the open Settings modal"
    );
}

#[gpui::test]
async fn hardware_details_scroll_when_sections_exceed_the_viewport(cx: &mut TestAppContext) {
    let window = root(cx);
    let view = window.entity(cx).expect("RootView window entity");
    view.update(cx, |view, cx| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::System;
        view.dashboard.section = SystemSection::Hardware;
        cx.notify();
    });
    cx.simulate_window_resize(window.into(), gpui::size(gpui::px(720.0), gpui::px(360.0)));
    draw(cx, window);
    view.read_with(cx, |view, _cx| {
        assert!(
            view.system_scroll.max_offset().height > gpui::px(0.0),
            "hardware details must scroll when the section cards exceed the viewport"
        );
        assert!(
            view.system_scroll.bounds().size.height <= gpui::px(360.0),
            "the hardware scroll viewport must stay inside the page"
        );
    });
}
