use super::{Switch, SwitchEvent, SwitchState};
use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, Modifiers, ParentElement, Render,
    ScrollHandle, StatefulInteractiveElement, Styled, TestAppContext, Window, div, px,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::Theme;
struct Harness {
    state: Entity<SwitchState>,
    events: Rc<RefCell<Vec<SwitchEvent>>>,
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let events = self.events.clone();
        div().child(
            Switch::new(self.state.clone(), Theme::dark().palette())
                .on_change(move |on, _, _| events.borrow_mut().push(SwitchEvent::Toggled { on })),
        )
    }
}

/// Space on the focused switch toggles it and emits the typed event.
#[gpui::test]
async fn keyboard_space_toggles_switch(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::<SwitchEvent>::new()));
    let window = cx.add_window(|_window, cx| {
        let state = cx.new(|cx| SwitchState::new(cx));
        Harness {
            state,
            events: events.clone(),
        }
    });
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear());

    let bounds = vcx
        .debug_bounds("tm-switch")
        .expect("switch must be laid out");
    assert!(f32::from(bounds.size.width) > 0.0);

    // Focus the switch via its state handle, then press Space.
    let handle = window
        .read_with(&vcx, |harness, app| {
            harness.state.read(app).focus_handle().clone()
        })
        .unwrap();
    vcx.update(|window, _| handle.focus(window));
    vcx.simulate_keystrokes("space");
    assert_eq!(
        events.borrow().as_slice(),
        &[SwitchEvent::Toggled { on: true }],
        "space must toggle the switch on"
    );

    // The state entity flipped too.
    let on = window
        .read_with(&vcx, |harness, app| harness.state.read(app).on)
        .unwrap();
    assert!(on);
}

/// Enter also toggles; state is the single source of truth.
#[gpui::test]
async fn keyboard_enter_toggles_switch(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::<SwitchEvent>::new()));
    let window = cx.add_window(|_window, cx| {
        let state = cx.new(|cx| SwitchState::new(cx));
        Harness {
            state,
            events: events.clone(),
        }
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear());
    let handle = window
        .read_with(&vcx, |harness, app| {
            harness.state.read(app).focus_handle().clone()
        })
        .unwrap();
    vcx.update(|window, _| handle.focus(window));
    vcx.simulate_keystrokes("enter");
    vcx.simulate_keystrokes("enter");

    assert_eq!(
        events.borrow().as_slice(),
        &[
            SwitchEvent::Toggled { on: true },
            SwitchEvent::Toggled { on: false },
        ],
        "enter must toggle twice (on then off)"
    );
    let on = window
        .read_with(&vcx, |harness, app| harness.state.read(app).on)
        .unwrap();
    assert!(!on);
}

/// Pointer click toggles through the same state path.
#[gpui::test]
async fn pointer_click_toggles_switch(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::<SwitchEvent>::new()));
    let window = cx.add_window(|_window, cx| {
        let state = cx.new(|cx| SwitchState::new(cx));
        Harness {
            state,
            events: events.clone(),
        }
    });
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);

    // Pointer path: click the switch's laid-out bounds.
    let center = vcx
        .debug_bounds("tm-switch")
        .expect("switch must be laid out")
        .center();
    vcx.simulate_click(center, Modifiers::none());
    assert_eq!(
        events.borrow().as_slice(),
        &[SwitchEvent::Toggled { on: true }],
        "pointer click must toggle the switch on"
    );
}

/// The switch is a REAL GPUI tab stop: `focus_next` from an unfocused
/// window reaches it (gpui's `TabStopMap` reads the tracked handle's own
/// `tab_stop` field — a plain `cx.focus_handle()` is never reachable).
#[gpui::test]
async fn switch_is_reachable_through_window_tab_navigation(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::<SwitchEvent>::new()));
    let window = cx.add_window(|_window, cx| {
        let state = cx.new(|cx| SwitchState::new(cx));
        Harness {
            state,
            events: events.clone(),
        }
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    window
        .update(cx, |_view, window, _cx| window.focus_next())
        .unwrap();
    assert!(
        window
            .update(cx, |_view, window, cx| window.focused(cx).is_some())
            .unwrap(),
        "focus_next must reach the switch"
    );
    cx.dispatch_keystroke(window.into(), gpui::Keystroke::parse("space").unwrap());
    assert_eq!(
        events.borrow().as_slice(),
        &[SwitchEvent::Toggled { on: true }],
        "space after focus_next must toggle the switch"
    );
}

/// A disabled switch must NOT be reachable through tab navigation: its
/// tracked handle carries `tab_stop(false)`, so `focus_next` skips it.
#[gpui::test]
async fn disabled_switch_is_excluded_from_tab_navigation(cx: &mut TestAppContext) {
    struct DisabledHarness {
        state: Entity<SwitchState>,
    }

    impl Render for DisabledHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            Switch::new(self.state.clone(), Theme::dark().palette()).disabled(true)
        }
    }

    let window = cx.add_window(|_window, cx| {
        let state = cx.new(|cx| SwitchState::new(cx));
        DisabledHarness { state }
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    window
        .update(cx, |_view, window, _cx| window.focus_next())
        .unwrap();
    assert!(
        !window
            .update(cx, |_view, window, cx| window.focused(cx).is_some())
            .unwrap(),
        "a disabled switch must not become a tab stop"
    );
}

struct ScrollingHarness {
    state: Entity<SwitchState>,
    scroll: ScrollHandle,
}
impl Render for ScrollingHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("switch-parent")
            .w(px(200.0))
            .h(px(100.0))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().h(px(600.0)).flex_none())
                    .child(Switch::new(self.state.clone(), Theme::dark().palette())),
            )
    }
}
#[gpui::test]
async fn focused_switch_reveals_itself_inside_its_real_scroll_viewport(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, cx| {
        let scroll = ScrollHandle::new();
        let state = cx.new(|cx| {
            let mut state = SwitchState::new(cx);
            state.set_scroll_parent(scroll.clone());
            state
        });
        ScrollingHarness { state, scroll }
    });
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear());
    let handle = window
        .read_with(&vcx, |view, app| {
            view.state.read(app).focus_handle().clone()
        })
        .unwrap();
    vcx.update(|window, _cx| handle.focus(window));
    for _ in 0..3 {
        vcx.update(|window, cx| window.draw(cx).clear());
    }
    window
        .read_with(&vcx, |view, app| {
            let state = view.state.read(app);
            let control = state.painted_bounds().expect("measured control");
            let viewport = view.scroll.bounds();
            assert!(control.size.width > px(0.0) && control.size.height > px(0.0));
            assert!(
                viewport.contains(&control.origin) && viewport.contains(&control.bottom_right()),
                "{control:?} outside {viewport:?}"
            );
            assert!(
                view.scroll.offset().y < px(0.0),
                "focus moves the actual scroll offset"
            );
            assert!(!state.is_on(), "reveal does not activate a switch");
        })
        .unwrap();
}
