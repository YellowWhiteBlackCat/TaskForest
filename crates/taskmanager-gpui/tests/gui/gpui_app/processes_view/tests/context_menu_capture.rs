//! Real-pixel context-menu capture scenarios for the process table.
//!
//! These tests drive the production right-click path and the capture
//! coordinator; they live apart from the general table behavior suite so the
//! source-line budget stays within the repository's hard limit.

use super::*;

/// Open the same `RootView` at an explicit window size for compact-layout
/// scenarios whose anchor must stay inside a narrower viewport.
fn wrapped_root_sized(
    cx: &mut TestAppContext,
    window_size: gpui::Size<Pixels>,
) -> (WindowHandle<RootView>, Entity<RootView>) {
    let win = cx.open_window(window_size, |_window, cx| RootView::new(Theme::dark(), cx));
    let view = win.entity(cx).expect("window root RootView entity");
    (win, view)
}

#[gpui::test]
async fn process_row_right_click_opens_the_real_context_menu(cx: &mut TestAppContext) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.set_process_sort(SortCol::Cpu, SortDir::Desc);
        v.replace_processes_for_test(vec![
            ProcessItemFixtureBuilder::new()
                .pid(101)
                .name("menu-worker".into())
                .build(),
        ]);
        cx.notify();
    });
    draw(cx, win);

    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let row = vcx
        .debug_bounds("tm-proc-row-root:1")
        .expect("expanded category process row renders");
    vcx.simulate_mouse_move(row.center(), None::<MouseButton>, Modifiers::none());
    vcx.simulate_mouse_down(row.center(), MouseButton::Right, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let popup = vcx
        .debug_bounds("tm-popup")
        .expect("the process row's production right-click listener opens the popup");
    assert!(
        popup.size.width >= px(120.0) && popup.size.height >= px(100.0),
        "the process menu must render its real action card, got {popup:?}"
    );
}

#[gpui::test]
async fn process_context_menu_capture_coordinator_opens_and_certifies_the_real_popup(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_root(cx);
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.install_process_context_menu_capture_for_test();
        v.page = TopPage::Apps;
        v.set_process_sort(SortCol::Cpu, SortDir::Desc);
        v.replace_processes_for_test(vec![
            ProcessItemFixtureBuilder::new()
                .pid(101)
                .name("capture-menu-worker".into())
                .build(),
        ]);
        cx.notify();
    });

    draw(cx, win);
    draw(cx, win);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let target_row = vcx
        .debug_bounds("tm-proc-row-root:1")
        .expect("the process row with identity renders");
    for _ in 0..16 {
        cx.update_window(win.into(), |_, window, cx| window.simulate_next_frame(cx))
            .unwrap();
        draw(cx, win);
        if view.read_with(cx, |v, _| v.process_context_menu_capture_ready()) {
            break;
        }
    }
    assert!(
        view.read_with(cx, |v, _| v.process_context_menu_capture_open()),
        "the capture coordinator must open the production context menu"
    );
    assert!(
        view.read_with(cx, |v, _| v.process_context_menu_capture_ready()),
        "the menu must survive its two-frame presentation gate"
    );
    let anchor = view
        .read_with(cx, |v, _| v.process_context_menu_capture_anchor())
        .expect("the coordinator records the dispatched pointer position");
    assert!(
        target_row.contains(&anchor),
        "the coordinator must dispatch onto the identity row, got anchor {anchor:?} \
         outside row {target_row:?}"
    );
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let popup = vcx
        .debug_bounds("tm-popup")
        .expect("the certified capture frame must paint the real popup");
    assert!(
        popup.size.width >= px(120.0) && popup.size.height >= px(100.0),
        "the capture must certify a full action card, got {popup:?}"
    );
    // The shared popup caps its scroll body at 360px, so a menu whose content
    // exceeds that scrolls rather than painting every entry. What this capture
    // must prove is that the panel itself stays inside the window edge.
    assert!(
        popup.right() <= px(1920.0) && popup.bottom() <= px(1080.0),
        "the certified menu panel must stay inside the window, got {popup:?}"
    );
}

/// The compact capture window (720x480) is narrower than the process table's
/// intrinsic content width. The coordinator must clamp the row anchor to the
/// visible part of the row instead of anchoring on the off-screen row center,
/// otherwise the guard rejects it and the scenario never certifies.
#[gpui::test]
async fn process_context_menu_capture_clamps_anchor_inside_a_compact_viewport(
    cx: &mut TestAppContext,
) {
    let (win, view) = wrapped_root_sized(cx, size(px(720.0), px(480.0)));
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.install_process_context_menu_capture_for_test();
        v.page = TopPage::Apps;
        v.set_process_sort(SortCol::Cpu, SortDir::Desc);
        v.replace_processes_for_test(vec![
            ProcessItemFixtureBuilder::new()
                .pid(101)
                .name("compact-capture-worker".into())
                .build(),
        ]);
        cx.notify();
    });

    draw(cx, win);
    draw(cx, win);
    for _ in 0..16 {
        cx.update_window(win.into(), |_, window, cx| window.simulate_next_frame(cx))
            .unwrap();
        draw(cx, win);
        if view.read_with(cx, |v, _| v.process_context_menu_capture_ready()) {
            break;
        }
    }
    assert!(
        view.read_with(cx, |v, _| v.process_context_menu_capture_ready()),
        "the compact capture must certify the real popup"
    );
    let anchor = view
        .read_with(cx, |v, _| v.process_context_menu_capture_anchor())
        .expect("the coordinator records the dispatched pointer position");
    assert!(
        anchor.x > px(0.0) && anchor.x < px(720.0) && anchor.y > px(0.0) && anchor.y < px(480.0),
        "the compact anchor must stay inside the 720x480 viewport, got {anchor:?}"
    );
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let popup = vcx
        .debug_bounds("tm-popup")
        .expect("the certified compact frame must paint the real popup");
    assert!(
        popup.size.width >= px(120.0) && popup.size.height >= px(100.0),
        "the compact capture must certify a full action card, got {popup:?}"
    );
    assert!(
        popup.right() <= px(720.0) && popup.bottom() <= px(480.0),
        "the compact menu panel must stay inside the window, got {popup:?}"
    );
}
