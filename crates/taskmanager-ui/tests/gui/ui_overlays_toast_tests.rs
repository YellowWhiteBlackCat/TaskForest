use super::{Toast, ToastEvent, ToastState};
use crate::primitives::card_surface::CardSurface;
use crate::theme_binding::{glass_fill, glass_shadow};
use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, Modifiers, ParentElement, Render,
    Styled, TestAppContext, Window, div, px, size,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use taskmanager_theme::GlassSurfaceRole;
use taskmanager_theme::GlassTier;
use taskmanager_theme::Theme;
use zhuge_ui_tokens::StatusKind;

/// The toast card's material is the shared floating Card-tier surface, not a
/// local fill + border + shadow recipe.
#[test]
fn the_toast_card_paints_the_shared_surface_contract() {
    let theme = Theme::dark();
    let mut owned = CardSurface::unpadded_for_role(theme.palette(), GlassSurfaceRole::Toast)
        .radius(theme.palette().panel_radius)
        .render();
    assert_eq!(
        owned.style().background.as_ref(),
        Some(&glass_fill(&theme, GlassTier::Card))
    );
    assert_eq!(
        owned.style().box_shadow.as_ref(),
        Some(&glass_shadow(&theme, GlassTier::Card))
    );
}

/// The toast's ink is resolved through the theme's own status ladder: the
/// shared ladder owns which severity takes which ink, so this crate keeps no
/// private copy of that mapping.
#[test]
fn toast_kind_colors_are_palette_derived() {
    let palette = Theme::dark().palette();
    let colors = palette.status_colors();
    assert_eq!(colors.resolve(StatusKind::Success), palette.success);
    assert_eq!(colors.resolve(StatusKind::Warning), palette.warning);
    assert_eq!(colors.resolve(StatusKind::Danger), palette.danger);
    assert_eq!(colors.resolve(StatusKind::Info), palette.accent);
}

/// Arm records the duration; the auto-dismiss timer is driven by the
/// host executor at runtime (the event path is exercised headlessly via
/// the emitter below).
#[gpui::test]
async fn auto_dismiss_arms_duration(cx: &mut TestAppContext) {
    let toast = cx.new(|cx| ToastState::new(7, "done", StatusKind::Success, cx));
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
    let toast = cx.new(|cx| ToastState::new(9, "bye", StatusKind::Danger, cx));
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
    let toast = cx.new(|cx| ToastState::new(14, "later", StatusKind::Warning, cx));
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
    let toast = cx.new(|cx| ToastState::new(12, "done", StatusKind::Success, cx));
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
        slot_width: 688.0,
        single_line: false,
        indicator: false,
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
    let toast = cx.new(|cx| ToastState::new(13, "done", StatusKind::Info, cx));
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
        slot_width: 688.0,
        single_line: false,
        indicator: false,
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

/// A toast is a bounded overlay, not an intrinsic-width banner. An operation
/// receipt may contain a full filesystem path; that path must wrap inside the
/// card instead of widening it past the viewport or displacing the page.
#[gpui::test]
async fn long_receipt_wraps_inside_the_toast_card(cx: &mut TestAppContext) {
    let path = format!(
        "/run/media/{}/very-long-workspace/target/host-wayland-diagnostic/runs/{}/capture.png",
        "capture-user", "977f44d34-bc94-4c7d-9d89-0f6a6b0a62e9"
    );
    let message = format!("Current window saved: {path} (1180\u{00d7}780)");
    let toast = cx.new(|cx| ToastState::new(15, message, StatusKind::Success, cx));
    let window = cx.add_window(|_window, _cx| ToastHarness {
        toast: toast.clone(),
        dismissals: Rc::new(RefCell::new(0)),
        slot_width: 688.0,
        single_line: false,
        indicator: false,
    });
    cx.simulate_window_resize(window.into(), size(px(720.0), px(480.0)));

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let container = vcx
        .debug_bounds("tm-toast-slot")
        .expect("the bounded toast slot must render");
    let card = vcx
        .debug_bounds("tm-toast-card")
        .expect("the toast card must render");

    assert!(
        card.size.width <= container.size.width,
        "the toast must stay inside its bounded slot: card={card:?} slot={container:?}"
    );
    assert!(
        card.size.height > px(40.0),
        "a long receipt must wrap to multiple lines instead of being clipped: {card:?}"
    );
}

/// The GPUI shell can also mount a receipt inside a one-line titlebar slot.
#[gpui::test]
async fn single_line_receipt_stays_inside_a_compact_chrome_slot(cx: &mut TestAppContext) {
    let path = format!(
        "/run/media/{}/very-long-workspace/target/host-wayland-diagnostic/runs/{}/capture.png",
        "capture-user", "977f44d34-bc94-4c7d-9d89-0f6a6b0a62e9"
    );
    let message = format!("Current window saved: {path} (1180\u{00d7}780)");
    let toast = cx.new(|cx| ToastState::new(16, message, StatusKind::Success, cx));
    let window = cx.add_window(|_window, _cx| ToastHarness {
        toast: toast.clone(),
        dismissals: Rc::new(RefCell::new(0)),
        slot_width: 254.0,
        single_line: true,
        indicator: false,
    });
    cx.simulate_window_resize(window.into(), size(px(720.0), px(480.0)));

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let slot = vcx
        .debug_bounds("tm-toast-slot")
        .expect("the compact chrome slot must render");
    let card = vcx
        .debug_bounds("tm-toast-card")
        .expect("the one-line toast card must render");

    assert!(
        card.size.width <= slot.size.width,
        "the one-line toast must fit its compact chrome slot: card={card:?} slot={slot:?}"
    );
    assert!(
        card.size.height <= px(46.0),
        "the one-line toast must fit a GNOME titlebar: {card:?}"
    );
}

#[gpui::test]
async fn indicator_toast_fits_a_compact_navigation_rail(cx: &mut TestAppContext) {
    let toast = cx.new(|cx| {
        ToastState::new(
            17,
            "Current window saved: /workspace/archive/capture.png",
            StatusKind::Success,
            cx,
        )
    });
    let window = cx.add_window(|_window, _cx| ToastHarness {
        toast: toast.clone(),
        dismissals: Rc::new(RefCell::new(0)),
        slot_width: 46.0,
        single_line: true,
        indicator: true,
    });
    cx.simulate_window_resize(window.into(), size(px(720.0), px(480.0)));

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let slot = vcx
        .debug_bounds("tm-toast-slot")
        .expect("the compact rail slot must render");
    let card = vcx
        .debug_bounds("tm-toast-card")
        .expect("the severity indicator must render");

    assert!(
        card.size.width <= slot.size.width,
        "the indicator must stay inside the navigation rail: card={card:?} slot={slot:?}"
    );
    assert!(
        vcx.debug_bounds("tm-toast-dismiss").is_none(),
        "the indicator must not paint a close target outside its narrow rail"
    );
}

struct ToastHarness {
    toast: Entity<ToastState>,
    dismissals: Rc<RefCell<usize>>,
    slot_width: f32,
    single_line: bool,
    indicator: bool,
}

impl Render for ToastHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let dismissals = self.dismissals.clone();
        let toast = Toast::new(self.toast.clone(), Theme::dark().palette());
        let toast = if self.single_line {
            toast.single_line()
        } else {
            toast
        };
        let toast = toast.indicator(self.indicator);
        // Mirror the production overlay's bounded width so the card is tested
        // against the slot it actually receives, not the full test window.
        div()
            .debug_selector(|| "tm-toast-slot".to_string())
            .w(px(self.slot_width))
            .h_full()
            .child(toast.on_dismiss(move |_, _| {
                *dismissals.borrow_mut() += 1;
            }))
    }
}
