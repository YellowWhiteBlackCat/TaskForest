//! Real GPUI scroll-chain tests for the Apps table.
//!
//! These tests intentionally dispatch `ScrollWheelEvent` into a rendered
//! RootView. Geometry-only tests cannot observe the ordering between the
//! uniform-list listener, the single horizontal owner, and the pinned header.

use gpui::{
    Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase, VisualTestContext, point,
    px, size,
};

use crate::gpui_app::root::TopPage;

fn wheel(
    position: gpui::Point<gpui::Pixels>,
    delta: gpui::Point<gpui::Pixels>,
    modifiers: Modifiers,
) -> ScrollWheelEvent {
    ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(delta),
        modifiers,
        touch_phase: TouchPhase::Moved,
    }
}

fn setup_apps(
    cx: &mut TestAppContext,
) -> (
    gpui::WindowHandle<crate::gpui_app::root::RootView>,
    gpui::Entity<crate::gpui_app::root::RootView>,
) {
    let (win, view) = super::wrapped_root(cx);
    cx.simulate_window_resize(win.into(), size(px(720.0), px(480.0)));
    view.update(cx, |v, cx| {
        v.mark_telemetry_frame_ready();
        v.page = TopPage::Apps;
        v.processes_state.hidden_cols.clear();
        v.replace_processes_for_test(
            (1..=120)
                .map(|pid| {
                    taskmanager_test_support::ProcessItemFixtureBuilder::new()
                        .pid(pid)
                        .name(format!("wheel-worker-{pid}"))
                        .build()
                })
                .collect(),
        );
        cx.notify();
    });
    super::draw(cx, win);
    (win, view)
}

fn offset_pair(
    view: &gpui::Entity<crate::gpui_app::root::RootView>,
    cx: &TestAppContext,
) -> (gpui::Point<gpui::Pixels>, gpui::Point<gpui::Pixels>) {
    view.read_with(cx, |v, _| {
        (
            v.processes_scroll.horizontal.offset(),
            v.processes_scroll.vertical.0.borrow().base_handle.offset(),
        )
    })
}

/// A real wheel over a body row must move only the vertical list. A native
/// horizontal/Shift-normalized wheel must move only the one horizontal owner.
/// This catches the old post-bubble guard, which ran after GPUI had already
/// cross-fed the y delta into x.
#[gpui::test]
async fn apps_wheel_keeps_horizontal_and_vertical_axes_independent(cx: &mut TestAppContext) {
    let (win, view) = setup_apps(cx);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let body = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("the Apps fixture must render a body row");
    vcx.debug_bounds("tm-proc-hdr-row")
        .expect("the Apps fixture must render a header row");

    let (h_before, v_before) = offset_pair(&view, cx);
    vcx.simulate_event(wheel(
        body.center(),
        point(px(0.0), px(-80.0)),
        Modifiers::none(),
    ));
    let (h_after_vertical, v_after_vertical) = offset_pair(&view, cx);
    assert_eq!(
        h_after_vertical.x, h_before.x,
        "a normal vertical wheel over rows must not move the horizontal owner"
    );
    assert!(
        v_after_vertical.y < v_before.y,
        "a normal vertical wheel must move the uniform-list owner"
    );

    super::draw(cx, win);
    let body = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("the body row remains rendered after vertical scrolling");
    vcx.simulate_event(wheel(
        body.center(),
        point(px(-80.0), px(0.0)),
        Modifiers::none(),
    ));
    let (h_after_horizontal, v_after_horizontal) = offset_pair(&view, cx);
    assert!(
        h_after_horizontal.x < h_after_vertical.x,
        "a horizontal wheel must move the single horizontal owner"
    );
    assert_eq!(
        v_after_horizontal.y, v_after_vertical.y,
        "a horizontal wheel must not perturb the vertical list"
    );

    super::draw(cx, win);
    let header = vcx
        .debug_bounds("tm-procs-header-scroll")
        .expect("the pinned header must remain in the horizontal viewport");
    let (_, v_before_header) = offset_pair(&view, cx);
    vcx.simulate_event(wheel(
        header.center(),
        point(px(0.0), px(-40.0)),
        Modifiers::none(),
    ));
    let (h_after_header, v_after_header) = offset_pair(&view, cx);
    assert_eq!(
        h_after_header.x, h_after_horizontal.x,
        "a vertical wheel over the header must not cross-feed into x"
    );
    assert!(
        v_after_header.y < v_before_header.y,
        "a vertical wheel over the pinned header must forward to the body list"
    );

    super::draw(cx, win);
    let body = vcx
        .debug_bounds("tm-proc-row-root:0")
        .expect("the body row must remain available for Shift-wheel");
    let (_, v_before_shift) = offset_pair(&view, cx);
    vcx.simulate_event(wheel(
        body.center(),
        point(px(-40.0), px(0.0)),
        Modifiers {
            shift: true,
            ..Modifiers::none()
        },
    ));
    let (h_after_shift, v_after_shift) = offset_pair(&view, cx);
    assert!(
        h_after_shift.x < h_after_header.x,
        "platform-normalized Shift-wheel must move x"
    );
    assert_eq!(
        v_after_shift.y, v_before_shift.y,
        "platform-normalized Shift-wheel must not move y"
    );
}

/// The header and body are two visible projections of one horizontal content
/// surface. After a real handle move and a fresh frame, both rendered bounds
/// must translate by the exact same amount; measuring only their initial
/// widths would miss a two-owner prepaint overwrite.
#[gpui::test]
async fn apps_header_and_body_follow_the_same_horizontal_offset(cx: &mut TestAppContext) {
    let (win, view) = setup_apps(cx);
    let mut vcx = VisualTestContext::from_window(win.into(), cx);
    let header_name_before = vcx
        .debug_bounds("tm-proc-h-sort-name")
        .expect("header Name cell must render");
    let body_name_before = vcx
        .debug_bounds("tm-proc-b-name")
        .expect("body Name cell must render");

    let shift = px(-96.0);
    view.update(cx, |v, _cx| {
        v.processes_scroll
            .horizontal
            .set_offset(point(shift, px(0.0)));
    });
    super::draw(cx, win);

    let header_name_after = vcx
        .debug_bounds("tm-proc-h-sort-name")
        .expect("header Name cell must remain rendered after horizontal scroll");
    let body_name_after = vcx
        .debug_bounds("tm-proc-b-name")
        .expect("body Name cell must remain rendered after horizontal scroll");
    let header_delta = f32::from(header_name_after.origin.x - header_name_before.origin.x);
    let body_delta = f32::from(body_name_after.origin.x - body_name_before.origin.x);
    assert!((header_delta - f32::from(shift)).abs() < 0.1);
    assert!((body_delta - f32::from(shift)).abs() < 0.1);
    assert!(
        (header_delta - body_delta).abs() < 0.1,
        "header/body must be translated by one shared owner (header_delta={header_delta}, body_delta={body_delta})"
    );
}
