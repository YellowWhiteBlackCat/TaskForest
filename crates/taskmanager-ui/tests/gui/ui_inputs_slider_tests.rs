use super::{Slider, SliderState, pointer_value};
use gpui::{
    AppContext, Bounds, Context, Entity, IntoElement, ParentElement, Render, TestAppContext,
    Window, div, point, px, size,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::Theme;
#[gpui::test]
async fn keyboard_steps_and_clamps(cx: &mut TestAppContext) {
    let events = Rc::new(RefCell::new(Vec::<f32>::new()));
    let window = cx.add_window(|_window, cx| {
        let state = cx.new(|cx| SliderState::new(0.0, 100.0, cx));
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
    vcx.simulate_keystrokes("right");
    assert_eq!(events.borrow().as_slice(), &[5.0]);

    // Home jumps to min, End to max.
    vcx.simulate_keystrokes("end");
    assert_eq!(*events.borrow().last().expect("event"), 100.0);
    vcx.simulate_keystrokes("home");
    assert_eq!(*events.borrow().last().expect("event"), 0.0);

    // Stepping past the end clamps.
    vcx.simulate_keystrokes("left");
    assert_eq!(*events.borrow().last().expect("event"), 0.0);

    let value = window
        .read_with(&vcx, |harness, app| harness.state.read(app).value())
        .unwrap();
    assert_eq!(value, 0.0);
}

/// State clamps out-of-range values (behavioral assertion).
#[gpui::test]
async fn state_clamps_values(cx: &mut TestAppContext) {
    let state = cx.new(|cx| SliderState::new(10.0, 20.0, cx));
    state.update(cx, |state, cx| {
        state.set_value(5.0, cx);
        assert_eq!(state.value(), 10.0);
        state.set_value(99.0, cx);
        assert_eq!(state.value(), 20.0);
        state.set_value(15.0, cx);
        assert_eq!(state.value(), 15.0);
    });
}

/// Pure pointer mapping clamps to the track.
#[test]
fn pointer_mapping_clamps() {
    let bounds = Bounds {
        origin: point(px(100.0), px(0.0)),
        size: size(px(200.0), px(28.0)),
    };
    assert_eq!(
        pointer_value(point(px(50.0), px(10.0)), &bounds, 0.0, 100.0),
        0.0
    );
    assert_eq!(
        pointer_value(point(px(400.0), px(10.0)), &bounds, 0.0, 100.0),
        100.0
    );
    assert_eq!(
        pointer_value(point(px(200.0), px(10.0)), &bounds, 0.0, 100.0),
        50.0
    );
}

struct Harness {
    state: Entity<SliderState>,
    events: Rc<RefCell<Vec<f32>>>,
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let events = self.events.clone();
        div().child(
            Slider::new(self.state.clone(), Theme::dark().palette())
                .on_change(move |value, _, _| events.borrow_mut().push(value)),
        )
    }
}
