use super::{SearchInput, SearchInputEvent};
use crate::inputs::text_input::TextInputState;
use gpui::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, TestAppContext, Window, div,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::Theme;
#[gpui::test]
async fn search_input_renders_and_cleans_on_escape(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::<SearchInputEvent>::new()));
    let window = cx.add_window(|_window, cx| {
        let state = cx.new(|cx| {
            let mut state = TextInputState::new(cx);
            state.set_clean_on_escape(true, cx);
            state
        });
        Harness {
            state,
            _events: events.clone(),
        }
    });
    cx.update(crate::inputs::text_input::init);
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
    vcx.update(|window, cx| window.draw(cx).clear());
    vcx.simulate_input("fire");
    let value = window
        .read_with(&vcx, |harness, app| {
            harness.state.read(app).value().to_string()
        })
        .unwrap();
    assert_eq!(value, "fire");

    // Escape with a non-empty query clears the field.
    vcx.simulate_keystrokes("escape");
    let value = window
        .read_with(&vcx, |harness, app| {
            harness.state.read(app).value().to_string()
        })
        .unwrap();
    assert!(value.is_empty());
}

struct Harness {
    state: Entity<TextInputState>,
    _events: Rc<RefCell<Vec<SearchInputEvent>>>,
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(
            SearchInput::new(self.state.clone(), Theme::dark().palette()).on_change(move |_, _| {}),
        )
    }
}
