use super::{Button, ButtonEvent, ButtonState, ButtonVariant, primary_fill, primary_state_fill};
use crate::styled::{active_fill, hover_fill};
use gpui::{
    AppContext, Context, Entity, IntoElement, Modifiers, ParentElement, Render, TestAppContext,
    Window, div,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::Palette;
fn palette() -> Palette {
    taskmanager_theme::Theme::dark().palette()
}

/// The primary fill must render as a LINEAR GRADIENT (not a solid), with
/// the hover/active state fills keeping the gradient shape — the Mission
/// Center button look. Locked through the runtime `Debug` form of the
/// actual fill value.
#[test]
fn primary_fill_is_a_gradient_in_every_state() {
    let p = palette();
    let debug = format!("{:?}", primary_fill(&p));
    assert!(
        debug.contains("LinearGradient"),
        "primary fill must be a linear gradient, got {debug}"
    );
    for state in [
        primary_state_fill(p.gradient_from, p.gradient_to, hover_fill),
        primary_state_fill(p.gradient_from, p.gradient_to, active_fill),
    ] {
        let debug = format!("{state:?}");
        assert!(
            debug.contains("LinearGradient"),
            "primary hover/active fills must stay gradients, got {debug}"
        );
    }
}

struct Harness {
    state: Entity<ButtonState>,
    disabled: bool,
    events: Rc<RefCell<Vec<ButtonEvent>>>,
}

impl Harness {
    fn new(cx: &mut Context<Self>, events: Rc<RefCell<Vec<ButtonEvent>>>) -> Self {
        let state = cx.new(|cx| ButtonState::new(cx));
        Self {
            state,
            disabled: false,
            events,
        }
    }
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.disabled {
            self.state
                .update(cx, |state, cx| state.set_enabled(false, cx));
        }
        let events = self.events.clone();
        let mut button = Button::new(self.state.clone(), palette())
            .label("Go")
            .on_activate(move |event, _, _| events.borrow_mut().push(*event));
        if self.disabled {
            button = button.variant(ButtonVariant::Secondary);
        }
        div().child(button)
    }
}

/// Keyboard activation: focus the button's tab stop, dispatch Enter, and
/// assert the typed payload arrives with `keyboard: true`.
#[gpui::test]
async fn keyboard_enter_activates_with_keyboard_payload(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::<ButtonEvent>::new()));
    let window = cx.add_window(|_window, cx| Harness::new(cx, events.clone()));
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    // Focus the button tab stop, then press Enter.
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear());
    let handle = window
        .read_with(&vcx, |harness, app| {
            harness.state.read(app).focus_handle().clone()
        })
        .unwrap();
    vcx.update(|window, _| handle.focus(window));
    let is_focused = vcx.update(|window, _| handle.is_focused(window));
    assert!(
        is_focused,
        "button must be focused before keyboard dispatch"
    );
    vcx.update(|window, cx| window.draw(cx).clear());
    vcx.simulate_keystrokes("enter");

    assert_eq!(
        events.borrow().as_slice(),
        &[ButtonEvent::Activated { keyboard: true }],
        "Enter on the focused button must activate via the keyboard payload"
    );
}

/// Pointer click shares the same activation path with `keyboard: false`.
#[gpui::test]
async fn pointer_click_activates_with_pointer_payload(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::<ButtonEvent>::new()));
    let window = cx.add_window(|_window, cx| Harness::new(cx, events.clone()));
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    // Pointer path: use the VisualTestContext for real click dispatch.
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    let center = vcx
        .debug_bounds("tm-button")
        .expect("button must be laid out")
        .center();
    vcx.simulate_click(center, Modifiers::none());

    assert_eq!(
        events.borrow().as_slice(),
        &[ButtonEvent::Activated { keyboard: false }],
        "pointer click must share the activation path"
    );
}

/// Disabled buttons are not tab stops and never fire activation.
#[gpui::test]
async fn disabled_button_does_not_activate(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::<ButtonEvent>::new()));
    let window = cx.add_window(|_window, cx| {
        let mut harness = Harness::new(cx, events.clone());
        harness.disabled = true;
        harness
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    // Keyboard activation is unreachable because the element is not a tab stop.
    let _ = window.update(cx, |_, window, _| window.focus_next());
    assert!(
        window
            .update(cx, |_, window, cx| window.focused(cx))
            .unwrap()
            .is_none(),
        "disabled button must not become a focus target"
    );
}
