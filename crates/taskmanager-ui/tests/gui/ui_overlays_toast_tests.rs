use super::{ToastEvent, ToastKind, ToastState};
use gpui::{AppContext, TestAppContext};
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

/// Subscriber-delivery lock for the toast's typed `Dismissed` payload.
///
/// The real auto-dismiss path awaits `smol::Timer`, which the headless test
/// clock cannot advance, so the timer→emit wiring cannot be driven here (see
/// `auto_dismiss_arms_duration` for the arming half). This test emits the
/// typed event directly and proves the payload reaches a subscriber carrying
/// the toast's id; it does not prove the timer fires it.
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
