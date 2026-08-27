use super::*;
use std::time::{Duration, Instant};

#[gpui::test]
async fn settings_modal_scrollbar_renders_with_valid_bounds(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        view.mark_telemetry_frame_ready();
        view.show_settings();
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = gpui::VisualTestContext::from_window(win.into(), cx);
    let scrollbar_bounds = vcx.debug_bounds("tm-settings-scrollbar");
    assert!(
        scrollbar_bounds.is_some(),
        "tm-settings-scrollbar must render in settings overlay"
    );
    let bounds = scrollbar_bounds.unwrap();
    assert!(
        bounds.size.height > px(50.0),
        "settings scrollbar must have real height: {:?}",
        bounds.size
    );
}

#[gpui::test]
async fn system_information_modal_uses_a_visible_fixed_rail_and_real_wheel(
    cx: &mut gpui::TestAppContext,
) {
    use gpui::{
        Modifiers, ScrollDelta, ScrollHandle, ScrollWheelEvent, TouchPhase, VisualTestContext,
        point, px, size,
    };

    let handle = ScrollHandle::new();
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    cx.simulate_window_resize(win.into(), size(px(900.0), px(520.0)));
    win.update(cx, |view, _window, cx| {
        view.mark_telemetry_frame_ready();
        view.show_system_about();
        view.dialog_scroll.system_about = handle.clone();
        let hardware = view.hardware_mut_for_test();
        hardware.os_name = Some("ExampleOS".into());
        hardware.os_version = Some("rolling".into());
        hardware.package_manager = Some("packages".into());
        hardware.package_manager_version = Some("1.0".into());
        hardware.package_count = Some(1_500);
        hardware.hostname = Some("host".into());
        hardware.shell = Some("/bin/fish".into());
        hardware.locale = Some("zh_CN.UTF-8".into());
        hardware.init_system = Some("systemd".into());
        hardware.kernel_version = Some("6.8".into());
        hardware.kernel_build = Some("a deliberately long kernel build identity".into());
        hardware.desktop_environment = Some("KDE Plasma".into());
        hardware.desktop_environment_version = Some("6.4".into());
        hardware.windowing_system = Some("wayland".into());
        hardware.window_manager = Some("KWin".into());
        hardware.compositor_backend = Some("Wayland".into());
        hardware.virtual_terminal = Some("tty2".into());
        hardware.cpu_brand = Some("Example CPU".into());
        hardware.cpu_cores = Some(16);
        hardware.total_memory_mb = Some(32_768);
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut visual = VisualTestContext::from_window(win.into(), cx);
    let viewport_before = visual
        .debug_bounds("tm-system-about-scroll")
        .expect("system information must expose its shared viewport");
    let rail_before = visual
        .debug_bounds("tm-system-about-scrollbar")
        .expect("system information must expose a visible pinned rail");
    let actions = visual
        .debug_bounds("tm-system-about-actions")
        .expect("system information must expose a fixed action row");
    let copy = visual
        .debug_bounds("tm-system-about-copy")
        .expect("copy all must remain a content-sized action");
    let first_title = visual
        .debug_bounds("tm-system-about-section-title-0")
        .expect("the first section title must remain addressable");
    let first_card = visual
        .debug_bounds("tm-system-about-section-card-0")
        .expect("the first section card must remain addressable");
    let first_value = visual
        .debug_bounds("system-about-selectable-value-0")
        .expect("system information values must use shared selectable text");
    assert!(handle.max_offset().height > px(0.0));
    assert!(copy.size.width < actions.size.width / 2.0);
    assert!(actions.bottom() < viewport_before.top());
    assert!(first_title.bottom() <= first_card.top());
    assert_eq!(rail_before.origin.y, viewport_before.origin.y);
    assert_eq!(rail_before.bottom(), viewport_before.bottom());
    assert_eq!(rail_before.right(), viewport_before.right());

    visual.simulate_click(first_value.center(), Modifiers::none());
    visual.simulate_keystrokes("ctrl-a ctrl-c");
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("ExampleOS"),
        "the integrated value must own selection/copy instead of triggering the row copy action"
    );

    let offset_before = handle.offset();
    visual.simulate_event(ScrollWheelEvent {
        position: viewport_before.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-80.0))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    assert!(handle.offset().y < offset_before.y);
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let viewport_after = visual
        .debug_bounds("tm-system-about-scroll")
        .expect("the viewport must remain mounted after scrolling");
    let rail_after = visual
        .debug_bounds("tm-system-about-scrollbar")
        .expect("the rail must remain mounted after scrolling");
    // The dialog entrance animation may move the whole panel by a fraction of
    // a pixel between redraws. The rail must retain its exact geometry relative
    // to that moving viewport; only the tracked content consumes the offset.
    assert_eq!(rail_after.size, rail_before.size);
    assert_eq!(
        rail_after.origin.x,
        viewport_after.right() - rail_after.size.width
    );
    assert_eq!(rail_after.origin.y, viewport_after.origin.y);
    assert_eq!(rail_after.bottom(), viewport_after.bottom());
}

#[gpui::test]
async fn cold_start_mask_tracks_the_typed_frame_lifecycle(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = gpui::VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-telemetry-warmup").is_some(),
        "Collecting must keep the warmup mask visible"
    );

    win.update(cx, |view, _window, cx| {
        view.mark_telemetry_frame_ready();
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    assert!(
        vcx.debug_bounds("tm-telemetry-ready-body").is_some(),
        "Ready must render the real page body"
    );
}

#[gpui::test]
async fn prolonged_warmup_exposes_a_focusable_retry_surface(cx: &mut gpui::TestAppContext) {
    let win = cx.add_window(|_window, cx| RootView::new(Theme::dark(), cx));
    win.update(cx, |view, _window, cx| {
        view.telemetry_warmup_started_at = Instant::now() - Duration::from_secs(20);
        cx.notify();
    })
    .unwrap();
    cx.update_window(win.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = gpui::VisualTestContext::from_window(win.into(), cx);
    assert!(
        vcx.debug_bounds("tm-telemetry-warmup").is_some(),
        "the warmup mask must remain visible while no complete frame exists"
    );
    assert!(
        vcx.debug_bounds("tm-telemetry-warmup-retry").is_some(),
        "a prolonged warmup must expose the retry control"
    );
}
