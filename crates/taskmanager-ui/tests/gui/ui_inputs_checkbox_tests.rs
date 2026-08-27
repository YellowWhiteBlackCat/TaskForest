use super::{Checkbox, CheckboxEvent, CheckboxState};
use gpui::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, TestAppContext, Window, div,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::Theme;
struct Harness {
    state: Entity<CheckboxState>,
    events: Rc<RefCell<Vec<CheckboxEvent>>>,
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let events = self.events.clone();
        div().child(
            Checkbox::new(self.state.clone(), Theme::dark().palette())
                .label("enable")
                .on_change(move |checked, _, _| {
                    events.borrow_mut().push(CheckboxEvent::Toggled { checked })
                }),
        )
    }
}

/// Space on the focused checkbox toggles it and emits the typed event.
#[gpui::test]
async fn keyboard_space_toggles_checkbox(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::<CheckboxEvent>::new()));
    let window = cx.add_window(|_window, cx| {
        let state = cx.new(|cx| CheckboxState::new(cx));
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
    vcx.simulate_keystrokes("space");
    assert_eq!(
        events.borrow().as_slice(),
        &[CheckboxEvent::Toggled { checked: true }],
        "space must check the box"
    );
    let checked = window
        .read_with(&vcx, |harness, app| harness.state.read(app).is_checked())
        .unwrap();
    assert!(checked);
}
