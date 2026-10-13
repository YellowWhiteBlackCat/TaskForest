use super::SelectOption;
use crate::inputs::select::select;
use gpui::{
    AppContext, Context, Modifiers, ParentElement, Render, Styled, TestAppContext, Window, div,
    point, px,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::Theme;

struct Harness {
    chosen: Rc<RefCell<Vec<String>>>,
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let palette = Theme::dark().palette();
        let chosen = self.chosen.clone();
        div().size_full().child(select(
            "mode",
            Some("auto".into()),
            "pick",
            vec![
                SelectOption::new("auto", "Automatic"),
                SelectOption::new("manual", "Manual"),
            ],
            palette,
            move |value, _window, _cx| {
                chosen.borrow_mut().push(value.to_string());
            },
        ))
    }
}

#[gpui::test]
async fn select_shows_current_value_and_reports_choice(cx: &mut TestAppContext) {
    cx.update(crate::overlays::popup::init);
    let chosen = Rc::new(RefCell::new(Vec::new()));
    let window = cx.add_window(|_window, _cx| Harness {
        chosen: chosen.clone(),
    });

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    cx.update_window(window.into(), |_, window, _| window.activate_window())
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let tb = vcx
        .debug_bounds("mode:trigger")
        .expect("trigger registered");
    assert!(tb.size.width > px(0.0), "the trigger must lay out");
    // The first draw pinned the popup anchor to the trigger; draw once
    // more so the trigger's element state is settled before the click
    // (the anchor write marks the state entity dirty for one frame).
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();

    // Open the menu by clicking the trigger (top-left, above the popup).
    let trigger_bounds = vcx.debug_bounds("mode:trigger").expect("trigger bounds");
    vcx.simulate_mouse_move(
        trigger_bounds.center(),
        None::<gpui::MouseButton>,
        Modifiers::none(),
    );
    vcx.simulate_click(trigger_bounds.center(), Modifiers::none());
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    assert!(
        vcx.debug_bounds("tm-popup").is_some(),
        "clicking the trigger must open the select menu"
    );

    // Pick the first option: hover then click the first popup row.
    let popup = vcx.debug_bounds("tm-popup").unwrap();
    let first_item = point(popup.left() + px(40.0), popup.top() + px(17.0));
    vcx.simulate_mouse_move(first_item, None::<gpui::MouseButton>, Modifiers::none());
    vcx.simulate_click(first_item, Modifiers::none());
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    assert_eq!(
        chosen.borrow().as_slice(),
        &[String::from("auto")],
        "selecting an option reports its value"
    );

    // `auto` was already checked before activation. Its display marker must
    // not turn this single-choice Select into a persistent checkbox menu.
    // The debug bounds map retains old entries, so click the old row again
    // and prove it cannot invoke the selection callback after dismissal.
    vcx.simulate_click(first_item, Modifiers::none());
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    assert_eq!(
        chosen.borrow().as_slice(),
        &[String::from("auto")],
        "choosing a checked single-select option must close the menu"
    );

    // Reopen the dismissed Select, then choose the other option through the
    // popup's keyboard navigation. The pointer anchor remains stable across
    // the first popup's unmount, while debug bounds retain historical entries.
    vcx.simulate_click(trigger_bounds.center(), Modifiers::none());
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    vcx.simulate_keystrokes("down down enter");
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    assert_eq!(
        chosen.borrow().as_slice(),
        &[String::from("auto"), String::from("manual")],
        "the trigger must regain focus and the reopened Select must accept another option"
    );
}
