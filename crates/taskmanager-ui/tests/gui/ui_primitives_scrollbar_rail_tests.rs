use super::{SCROLLBAR_WIDTH, ScrollbarHandle, ScrollbarRail, thumb_id};
use gpui::{
    AppContext, Bounds, Context, ElementId, InteractiveElement, IntoElement, Modifiers,
    MouseButton, ParentElement, Pixels, Point, Render, ScrollDelta, ScrollHandle, ScrollWheelEvent,
    Size, StatefulInteractiveElement, Styled, TestAppContext, TouchPhase, Window, div, point, px,
    size,
};
use std::cell::Cell;
use std::rc::Rc;
use taskmanager_theme::{Theme, tokens};

/// Scroll handle stub with a fixed content size that records every
/// offset write (the rail's click path moves it).
#[derive(Default)]
struct RecordingHandle {
    content: Size<Pixels>,
    offset: Cell<Point<Pixels>>,
    drag_starts: Cell<usize>,
    drag_ends: Cell<usize>,
}

impl ScrollbarHandle for RecordingHandle {
    fn offset(&self) -> Point<Pixels> {
        self.offset.get()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.offset.set(offset);
    }

    fn max_offset(&self) -> Size<Pixels> {
        size(
            (self.content.width - px(200.0)).max(px(0.0)),
            (self.content.height - px(300.0)).max(px(0.0)),
        )
    }

    fn viewport(&self) -> Bounds<Pixels> {
        Bounds::new(point(px(0.0), px(0.0)), size(px(200.0), px(300.0)))
    }

    fn start_drag(&self) {
        self.drag_starts.set(self.drag_starts.get() + 1);
    }

    fn end_drag(&self) {
        self.drag_ends.set(self.drag_ends.get() + 1);
    }
}

/// A 200×300 relative box hosting one rail pinned to its right edge;
/// `track_keyed` opts the thin track into `debug_bounds`.
struct Harness {
    handle: Rc<RecordingHandle>,
    track_keyed: bool,
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let rail = ScrollbarRail::vertical(
            "test-rail",
            "tm-test-rail",
            self.handle.clone(),
            Theme::dark().palette(),
        );
        let rail = if self.track_keyed {
            rail.track_debug_selector("tm-test-rail-track")
        } else {
            rail
        };
        div().relative().w(px(200.0)).h(px(300.0)).child(rail)
    }
}

fn recording_handle(content_height: f32) -> Rc<RecordingHandle> {
    Rc::new(RecordingHandle {
        content: size(px(200.0), px(content_height)),
        offset: Cell::new(point(px(0.0), px(0.0))),
        drag_starts: Cell::new(0),
        drag_ends: Cell::new(0),
    })
}

fn add_harness(
    cx: &mut TestAppContext,
    handle: Rc<RecordingHandle>,
    track_keyed: bool,
) -> gpui::WindowHandle<Harness> {
    cx.add_window(move |_window, _cx| Harness {
        handle: handle.clone(),
        track_keyed,
    })
}

/// A real GPUI scroll owner under the rail. This is intentionally different
/// from `RecordingHandle`: the test must exercise GPUI's hit-test and scroll
/// listener chain, not merely prove that a scrollbar callback can mutate a
/// fake offset.
struct WheelHarness {
    handle: ScrollHandle,
}

impl Render for WheelHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = div()
            .id("wheel-viewport")
            .w(px(200.0))
            .h(px(300.0))
            .overflow_y_scroll()
            .track_scroll(&self.handle)
            .child(div().w(px(200.0)).h(px(1200.0)));
        div()
            .relative()
            .w(px(200.0))
            .h(px(300.0))
            .child(viewport)
            .child(ScrollbarRail::vertical(
                "wheel-rail",
                "tm-wheel-rail",
                Rc::new(self.handle.clone()),
                Theme::dark().palette(),
            ))
    }
}

#[gpui::test]
async fn rail_renders_full_height_hit_strip_with_thin_centered_track(cx: &mut TestAppContext) {
    let window = add_harness(cx, recording_handle(1000.0), true);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    let rail = vcx
        .debug_bounds("tm-test-rail")
        .expect("rail wrapper must register its debug selector");
    let track = vcx
        .debug_bounds("tm-test-rail-track")
        .expect("opted-in track must register its debug selector");
    assert_eq!(
        rail.size.width,
        px(SCROLLBAR_WIDTH),
        "the hit strip is exactly the scrollbar hit width"
    );
    assert_eq!(
        rail.size.height,
        px(300.0),
        "top_0/bottom_0 pins the strip to the host height"
    );
    assert_eq!(track.size.width, px(1.0), "the visual track stays 1px");
    assert_eq!(
        track.origin.x + track.size.width,
        rail.origin.x + rail.size.width - px((SCROLLBAR_WIDTH - 1.0) / 2.0),
        "the track centers on the strip's midline"
    );
    let inset = px(tokens::SPACE_4.0);
    assert_eq!(track.origin.y, rail.origin.y + inset);
    assert_eq!(
        track.origin.y + track.size.height,
        rail.origin.y + rail.size.height - inset,
        "the track insets SPACE_4 from both ends"
    );
}

#[gpui::test]
async fn rail_registers_no_track_selector_by_default(cx: &mut TestAppContext) {
    let window = add_harness(cx, recording_handle(1000.0), false);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    assert!(
        vcx.debug_bounds("tm-test-rail")
            .is_some_and(|rail| rail.size.width == px(SCROLLBAR_WIDTH)),
        "the default rail still registers its wrapper selector"
    );
    assert!(
        vcx.debug_bounds("tm-test-rail-track").is_none(),
        "the track selector is opt-in"
    );
}

#[gpui::test]
async fn rail_click_repositions_scroll_only_when_content_overflows(cx: &mut TestAppContext) {
    let overflowing = recording_handle(1000.0);
    let fitting = recording_handle(300.0);
    let win_overflowing = add_harness(cx, overflowing.clone(), false);
    let win_fitting = add_harness(cx, fitting.clone(), false);

    let click_low_in_the_rail = |vcx: &mut gpui::VisualTestContext| {
        let rail = vcx
            .debug_bounds("tm-test-rail")
            .expect("rail wrapper must register its debug selector");
        let low = point(
            rail.left() + px(SCROLLBAR_WIDTH / 2.0),
            rail.top() + px(250.0),
        );
        vcx.simulate_mouse_move(low, None::<gpui::MouseButton>, Modifiers::none());
        vcx.simulate_click(low, Modifiers::none());
    };

    let mut vcx = gpui::VisualTestContext::from_window(win_overflowing.into(), cx);
    cx.update_window(win_overflowing.into(), |_, window, cx| {
        window.draw(cx).clear()
    })
    .unwrap();
    click_low_in_the_rail(&mut vcx);
    assert!(
        overflowing.offset.get().y < px(0.0),
        "a track click below the thumb must jump the handle offset into the \
             scrollable range (got {:?})",
        overflowing.offset.get()
    );

    let mut vcx = gpui::VisualTestContext::from_window(win_fitting.into(), cx);
    cx.update_window(win_fitting.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    click_low_in_the_rail(&mut vcx);
    assert_eq!(
        fitting.offset.get(),
        point(px(0.0), px(0.0)),
        "nothing to scroll: the rail must not move the offset"
    );
}

#[gpui::test]
async fn rail_drag_keeps_capture_until_release_outside_window(cx: &mut TestAppContext) {
    let handle = recording_handle(1000.0);
    let window = add_harness(cx, handle.clone(), false);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    let rail = vcx
        .debug_bounds("tm-test-rail")
        .expect("rail wrapper must register its debug selector");
    let thumb_center = point(
        rail.left() + px(SCROLLBAR_WIDTH / 2.0),
        rail.top() + px(48.0),
    );

    vcx.simulate_mouse_move(thumb_center, None::<MouseButton>, Modifiers::none());
    vcx.simulate_mouse_down(thumb_center, MouseButton::Left, Modifiers::none());
    assert_eq!(
        handle.drag_starts.get(),
        1,
        "thumb press must start one drag"
    );

    vcx.simulate_mouse_move(
        point(thumb_center.x, rail.bottom() + px(120.0)),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    assert_eq!(
        handle.offset.get().y,
        px(-700.0),
        "dragging beyond the rail must clamp to the real max offset"
    );

    vcx.simulate_mouse_up(
        point(px(2500.0), px(1500.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert_eq!(
        handle.drag_ends.get(),
        1,
        "capture release must end the drag"
    );
}

#[gpui::test]
async fn rail_hit_strip_keeps_wheel_dispatch_on_the_underlying_scroll_owner(
    cx: &mut TestAppContext,
) {
    let handle = ScrollHandle::new();
    let window = cx.add_window({
        let handle = handle.clone();
        move |_window, _cx| WheelHarness { handle }
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    assert!(
        handle.max_offset().height > px(0.0),
        "the fixture must have a real vertical scroll range"
    );
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    let rail = vcx
        .debug_bounds("tm-wheel-rail")
        .expect("the wheel fixture must expose the rail hit strip");
    let before = handle.offset();
    vcx.simulate_event(ScrollWheelEvent {
        position: rail.center(),
        delta: ScrollDelta::Pixels(point(px(0.0), px(-80.0))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    });
    assert!(
        handle.offset().y < before.y,
        "wheel over the rail must reach the underlying scroll owner (before={before:?}, after={:?})",
        handle.offset()
    );
}

#[test]
fn thumb_element_id_derives_from_wrapper_id() {
    for (wrapper, thumb) in [
        ("app-history-scrollbar", "app-history-scrollbar-thumb"),
        ("settings-scrollbar", "settings-scrollbar-thumb"),
    ] {
        let ElementId::Name(name) = thumb_id(wrapper) else {
            panic!("thumb id must be the name variant");
        };
        assert_eq!(name.as_ref(), thumb);
    }
}
