use super::DropdownMenu;
use gpui::{
    AppContext, Context, InteractiveElement, IntoElement, Modifiers, ParentElement, Render, Styled,
    TestAppContext, Window, div, point, px,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::Theme;

/// 附录 A-18 lock (absorption 3.6-7): the menu entity is cached in the
/// dropdown's element state — repeated draws/request_layouts never
/// rebuild it, and reopening the menu reuses the same entity instead of
/// rerunning the builder.
///
/// Observation (product code untouched): the open path is currently
/// unreachable — `request_layout` forwards the wrapped trigger with
/// `request_layout(None, …)` (dropping its element state/global id) and
/// the wrapper's `prepaint`/`paint` are no-ops, so the trigger's click
/// handlers never register and the menu can never be opened by input.
/// The builder therefore never runs while closed; the "reopen reuses
/// the cached entity" half of the contract cannot be exercised
/// end-to-end until the wrapper forwards the wrapped element's lifecycle.
#[gpui::test]
async fn menu_entity_is_cached_across_draws_and_reopens(cx: &mut TestAppContext) {
    let builds = Rc::new(RefCell::new(0));
    let window = cx.add_window(|_window, _cx| DropdownHarness {
        builds: builds.clone(),
    });

    // Many draws (request_layout every frame): the builder must never
    // run while the menu is closed — no entity churn, no popup.
    for _ in 0..4 {
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
    }
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    assert_eq!(
        *builds.borrow(),
        0,
        "closed draws must never build the menu entity"
    );
    assert_eq!(
        vcx.debug_bounds("tm-popup"),
        None,
        "no popup may render while closed"
    );

    // The wrapper now forwards the wrapped element's lifecycle
    // (GlobalElementId + prepaint/paint), so the trigger click reaches
    // the open path: the menu entity is built exactly once and the
    // popup renders. (Toggle-close and outside-dismiss of the popup are
    // covered by the popup's own dismiss tests.)
    vcx.simulate_click(point(px(960.0), px(540.0)), Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        *builds.borrow(),
        1,
        "trigger click must open the menu and build the entity exactly once"
    );
    assert!(
        vcx.debug_bounds("tm-popup").is_some(),
        "the popup must render once the menu is open"
    );

    // More draws while open: the cached entity is reused, no rebuilds.
    for _ in 0..2 {
        vcx.update(|window, cx| window.draw(cx).clear());
    }
    assert_eq!(
        *builds.borrow(),
        1,
        "open draws must keep reusing the cached menu entity"
    );
}

#[gpui::test]
async fn dropdown_renders(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| Harness);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let _ = window.read_with(cx, |_, _| {});
}

struct DropdownHarness {
    builds: Rc<RefCell<usize>>,
}

impl Render for DropdownHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::dark().palette();
        let builds = self.builds.clone();
        div().size_full().child(DropdownMenu::new(
            "dd",
            div().id("dd-trigger").size_full(),
            palette,
            move |state, _cx| {
                *builds.borrow_mut() += 1;
                state
            },
        ))
    }
}

#[derive(Default)]
struct Harness;

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::dark().palette();
        div().child(DropdownMenu::new(
            "dd",
            div().id("trigger").w_24().h_8(),
            palette,
            |state, _| state,
        ))
    }
}
