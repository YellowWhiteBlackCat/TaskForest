use super::{IconButton, IconButtonEvent, IconButtonState};
use gpui::{
    AppContext, Context, Entity, IntoElement, Modifiers, ParentElement, Render, TestAppContext,
    Window, div,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::Theme;
use taskmanager_ui_contract::IconId;
struct Harness {
    state: Entity<IconButtonState>,
    events: Rc<RefCell<Vec<IconButtonEvent>>>,
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let events = self.events.clone();
        div().child(
            IconButton::new(self.state.clone(), IconId::Refresh, Theme::dark().palette())
                .on_activate(move |event, _, _| events.borrow_mut().push(*event)),
        )
    }
}

#[gpui::test]
async fn icon_button_keyboard_and_pointer_share_activation(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::<IconButtonEvent>::new()));
    let window = cx.add_window(|_window, cx| {
        let state = cx.new(|cx| IconButtonState::new(cx));
        Harness {
            state,
            events: events.clone(),
        }
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    // Keyboard activation.
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear());
    let handle = window
        .read_with(&vcx, |harness, app| {
            harness.state.read(app).focus_handle().clone()
        })
        .unwrap();
    vcx.update(|window, _| handle.focus(window));
    vcx.simulate_keystrokes("space");
    assert_eq!(
        events.borrow().as_slice(),
        &[IconButtonEvent::Activated { keyboard: true }],
        "space on the focused icon button must activate via the keyboard payload"
    );

    // Pointer activation shares the path.
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    let center = vcx
        .debug_bounds("tm-icon-button")
        .expect("icon button must be laid out")
        .center();
    vcx.simulate_click(center, Modifiers::none());
    assert_eq!(
        events.borrow().as_slice(),
        &[
            IconButtonEvent::Activated { keyboard: true },
            IconButtonEvent::Activated { keyboard: false },
        ],
        "pointer click must share the activation path"
    );
}
