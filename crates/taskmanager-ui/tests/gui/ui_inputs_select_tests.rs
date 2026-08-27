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
    chosen: Rc<RefCell<Option<String>>>,
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
                *chosen.borrow_mut() = Some(value.to_string());
            },
        ))
    }
}

#[gpui::test]
async fn select_shows_current_value_and_reports_choice(cx: &mut TestAppContext) {
    let chosen = Rc::new(RefCell::new(None));
    let window = cx.add_window(|_window, _cx| Harness {
        chosen: chosen.clone(),
    });

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    cx.update_window(window.into(), |_, window, _| window.activate_window())
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let tb = vcx
        .debug_bounds("mode:trigger")
        .expect("trigger registered");
    assert!(tb.size.width > px(0.0), "the trigger must lay out");
    // The first draw pinned the popup anchor to the trigger; draw once
    // more so the trigger's element state is settled before the click
    // (the anchor write marks the state entity dirty for one frame).
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    // Open the menu by clicking the trigger (top-left, above the popup).
    let trigger_bounds = vcx.debug_bounds("mode:trigger").expect("trigger bounds");
    vcx.simulate_mouse_move(
        trigger_bounds.center(),
        None::<gpui::MouseButton>,
        Modifiers::none(),
    );
    vcx.simulate_click(trigger_bounds.center(), Modifiers::none());
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
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
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    assert_eq!(
        *chosen.borrow(),
        Some("auto".into()),
        "selecting an option reports its value"
    );
}
