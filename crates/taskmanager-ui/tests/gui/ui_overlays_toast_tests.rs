use super::{Toast, ToastEvent, ToastKind, ToastState};
use gpui::{
    AppContext, Context, Entity, IntoElement, Modifiers, ParentElement, Render, Styled,
    TestAppContext, Window, div,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use taskmanager_theme::Theme;
#[test]
fn toast_kind_colors_are_palette_derived() {
    let palette = Theme::dark().palette();
    assert_eq!(ToastKind::Success.color(&palette), palette.success);
    assert_eq!(ToastKind::Warning.color(&palette), palette.warning);
    assert_eq!(ToastKind::Danger.color(&palette), palette.danger);
    assert_eq!(ToastKind::Info.color(&palette), palette.accent);
}

/// Arm records the duration; the auto-dismiss timer is driven by the
/// host executor at runtime (the event path is exercised headlessly via
/// the emitter below).
#[gpui::test]
async fn auto_dismiss_arms_duration(cx: &mut TestAppContext) {
    let toast = cx.new(|cx| ToastState::new(7, "done", ToastKind::Success, cx));
    toast.update(cx, |toast, cx| {
        toast.arm_auto_dismiss(Duration::from_millis(10), cx);
        assert_eq!(toast.auto_dismiss(), Some(Duration::from_millis(10)));
        cx.notify();
    });
    let duration = toast.read_with(cx, |toast, _| toast.auto_dismiss());
    assert_eq!(duration, Some(Duration::from_millis(10)));
}

/// Subscriber-delivery lock for an explicitly emitted typed `Dismissed`
/// payload. The timer-driven production path is covered separately below.
#[gpui::test]
async fn synthetic_dismissed_event_reaches_subscribers_with_the_toast_id(cx: &mut TestAppContext) {
    let toast = cx.new(|cx| ToastState::new(9, "bye", ToastKind::Danger, cx));
    let received = Rc::new(RefCell::new(None::<ToastEvent>));
    let sink = received.clone();
    cx.update(|cx| {
        cx.subscribe(&toast, move |_, event: &ToastEvent, _cx| {
            *sink.borrow_mut() = Some(*event);
        })
        .detach();
    });

    toast.update(cx, |toast, cx| {
        cx.emit(ToastEvent::Dismissed { id: toast.id });
    });
    cx.run_until_parked();

    assert_eq!(
        *received.borrow(),
        Some(ToastEvent::Dismissed { id: 9 }),
        "the typed Dismissed event must reach subscribers with the toast id"
    );
}

/// The actual auto-dismiss timer emits exactly one typed event carrying the
/// armed toast's id. The outer `gpui::Timer` is a real wall-clock wait because
/// the TestDispatcher's virtual clock does not advance `smol::Timer`.
#[gpui::test]
async fn auto_dismiss_timer_emits_dismissed_event_with_its_toast_id(cx: &mut TestAppContext) {
    let toast = cx.new(|cx| ToastState::new(14, "later", ToastKind::Warning, cx));
    let events = Rc::new(RefCell::new(Vec::<ToastEvent>::new()));
    let event_sink = events.clone();
    cx.update(|cx| {
        cx.subscribe(&toast, move |_, event: &ToastEvent, _cx| {
            event_sink.borrow_mut().push(*event);
        })
        .detach();
    });

    toast.update(cx, |toast, cx| {
        toast.arm_auto_dismiss(Duration::from_millis(10), cx);
        assert_eq!(toast.auto_dismiss(), Some(Duration::from_millis(10)));
    });
    assert!(
        events.borrow().is_empty(),
        "arming must not dismiss eagerly"
    );

    let executor = cx.executor();
    executor.allow_parking();
    cx.run_until_parked();
    cx.executor().timer(Duration::from_millis(30)).await;
    executor.forbid_parking();
    cx.run_until_parked();

    assert_eq!(
        *events.borrow(),
        [ToastEvent::Dismissed { id: 14 }],
        "the real auto-dismiss timer must deliver exactly one event for this toast"
    );
}

#[gpui::test]
async fn dismiss_control_accepts_pointer_click_without_emitting_auto_dismiss(
    cx: &mut TestAppContext,
) {
    let toast = cx.new(|cx| ToastState::new(12, "done", ToastKind::Success, cx));
    let dismissals = Rc::new(RefCell::new(0));
    let events = Rc::new(RefCell::new(Vec::<ToastEvent>::new()));
    let event_sink = events.clone();
    cx.update(|cx| {
        cx.subscribe(&toast, move |_, event: &ToastEvent, _cx| {
            event_sink.borrow_mut().push(*event);
        })
        .detach();
    });
    let window = cx.add_window(|_window, _cx| ToastHarness {
        toast: toast.clone(),
        dismissals: dismissals.clone(),
    });

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let dismiss_bounds = vcx
        .debug_bounds("tm-toast-dismiss")
        .expect("dismiss control must render");
    vcx.simulate_click(dismiss_bounds.center(), Modifiers::none());

    assert_eq!(*dismissals.borrow(), 1, "pointer click must dismiss once");
    assert!(
        events.borrow().is_empty(),
        "manual dismissal must not impersonate the auto-dismiss timer event"
    );
}

#[gpui::test]
async fn dismiss_control_is_a_keyboard_tab_stop_and_activates_with_enter(cx: &mut TestAppContext) {
    let toast = cx.new(|cx| ToastState::new(13, "done", ToastKind::Info, cx));
    let dismissals = Rc::new(RefCell::new(0));
    let events = Rc::new(RefCell::new(Vec::<ToastEvent>::new()));
    let event_sink = events.clone();
    cx.update(|cx| {
        cx.subscribe(&toast, move |_, event: &ToastEvent, _cx| {
            event_sink.borrow_mut().push(*event);
        })
        .detach();
    });
    let window = cx.add_window(|_window, _cx| ToastHarness {
        toast: toast.clone(),
        dismissals: dismissals.clone(),
    });
    let focus_handle = toast.read_with(cx, |state, _| state.dismiss_focus.clone());

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        window.activate_window();
        window.draw(cx).clear(cx);
        window.focus_next(cx);
    });
    assert!(
        vcx.update(|window, _| focus_handle.is_focused(window)),
        "the toast dismiss control must be reachable by Tab focus"
    );
    vcx.simulate_keystrokes("enter");

    assert_eq!(*dismissals.borrow(), 1, "Enter must dismiss exactly once");
    assert!(
        events.borrow().is_empty(),
        "keyboard dismissal must not emit the auto-dismiss timer event"
    );
}

struct ToastHarness {
    toast: Entity<ToastState>,
    dismissals: Rc<RefCell<usize>>,
}

impl Render for ToastHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let dismissals = self.dismissals.clone();
        div().size_full().child(
            Toast::new(self.toast.clone(), Theme::dark().palette()).on_dismiss(move |_, _| {
                *dismissals.borrow_mut() += 1;
            }),
        )
    }
}
