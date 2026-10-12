use super::DropdownMenu;
use crate::overlays::popup::{MenuEntry, MenuItem, PopupMenuState};
use gpui::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    Modifiers, ParentElement, Render, StatefulInteractiveElement, Styled, TestAppContext, Window,
    div, point, px,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::Theme;

/// The open path builds one menu entity, and repeated renders while it stays
/// open reuse that entity instead of rerunning the builder.
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

/// Checked dropdown choices behave as toggles: each click updates its action,
/// keeps the same popup open for the next choice, and Escape restores focus to
/// the trigger so it can be activated again by keyboard.
#[gpui::test]
async fn checked_choices_stay_open_and_escape_restores_trigger_focus(cx: &mut TestAppContext) {
    cx.update(super::super::popup::init);
    let builds = Rc::new(RefCell::new(0));
    let toggles = Rc::new(RefCell::new(Vec::new()));
    let menu_focus = Rc::new(RefCell::new(None::<FocusHandle>));
    let dropdown_state_probe = Rc::new(RefCell::new(None));
    let window = cx.add_window(|_window, _cx| ToggleDropdownHarness {
        builds: builds.clone(),
        toggles: toggles.clone(),
        menu_focus: menu_focus.clone(),
        dropdown_state_probe: dropdown_state_probe.clone(),
    });
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        window.activate_window();
        window.draw(cx).clear();
    });

    let trigger = vcx
        .debug_bounds("dropdown-toggle-trigger")
        .expect("dropdown trigger must render");
    vcx.simulate_click(trigger.center(), Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    let popup = vcx.debug_bounds("tm-popup").expect("menu must open");
    assert!(
        vcx.update(|window, _cx| menu_focus
            .borrow()
            .as_ref()
            .is_some_and(|focus| focus.is_focused(window))),
        "opening the menu moves focus into its popup element"
    );
    let dropdown_state = dropdown_state_probe
        .borrow()
        .as_ref()
        .expect("the dropdown state probe is installed")
        .borrow()
        .as_ref()
        .expect("the rendered dropdown owns state")
        .clone();
    let trigger_focus = dropdown_state.read_with(&vcx, |state, _| state.focus_handle.clone());

    let first = point(popup.left() + px(40.0), popup.top() + px(17.0));
    let second = point(popup.left() + px(40.0), popup.top() + px(43.0));
    vcx.simulate_click(first, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    vcx.simulate_click(second, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        toggles.borrow().as_slice(),
        &[0, 1],
        "both checked choices must remain actionable in the same open menu"
    );
    assert_eq!(
        *builds.borrow(),
        1,
        "toggle clicks must reuse the open menu"
    );

    vcx.simulate_keystrokes("escape");
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        toggles.borrow().as_slice(),
        &[0, 1],
        "Escape must dismiss without activating another choice"
    );
    assert!(
        vcx.update(|window, _cx| menu_focus
            .borrow()
            .as_ref()
            .is_none_or(|focus| !focus.is_focused(window))),
        "Escape moves focus out of the popup element"
    );
    assert!(
        vcx.update(|window, _cx| trigger_focus.is_focused(window)),
        "Escape restores the exact FocusHandle registered on the trigger"
    );

    // A pointer activation provides a direct open-state check after Escape:
    // if the popup stayed mounted this would close it and leave builds at 1.
    vcx.simulate_click(trigger.center(), Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    assert!(
        vcx.update(|window, _cx| menu_focus
            .borrow()
            .as_ref()
            .is_some_and(|focus| focus.is_focused(window))),
        "reopening the menu restores focus to its new popup element"
    );
    assert_eq!(
        *builds.borrow(),
        2,
        "Escape must close the popup before the trigger can open it again"
    );
    vcx.simulate_keystrokes("escape");
    vcx.update(|window, cx| window.draw(cx).clear());
    assert!(
        vcx.update(|window, cx| window.focused(cx).is_some()),
        "Escape should leave focus on the dropdown trigger"
    );
    assert!(
        !dropdown_state.read_with(&vcx, |state, _| state.open),
        "Escape clears the dropdown's owning open state"
    );
    vcx.simulate_keystrokes("enter");
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        *builds.borrow(),
        3,
        "Enter after Escape must reopen the menu from restored trigger focus"
    );

    vcx.simulate_click(point(px(700.0), px(400.0)), Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        toggles.borrow().as_slice(),
        &[0, 1],
        "outside dismissal must not activate a checkable menu item"
    );
    vcx.simulate_keystrokes("enter");
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        *builds.borrow(),
        4,
        "outside dismissal must also restore focus to the dropdown trigger"
    );
}

/// Pin the GPUI reference's outside-press propagation: the menu closes while
/// the background control under that same click still receives activation.
#[gpui::test]
async fn outside_click_dismisses_dropdown_and_activates_background_target(cx: &mut TestAppContext) {
    cx.update(super::super::popup::init);
    let activations = Rc::new(RefCell::new(0));
    let window = cx.add_window(|_window, _cx| OutsideClickHarness {
        activations: activations.clone(),
    });
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear());

    let trigger = vcx
        .debug_bounds("outside-click-dropdown-trigger")
        .expect("dropdown trigger must render");
    vcx.simulate_click(trigger.center(), Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    assert!(
        vcx.debug_bounds("tm-popup").is_some(),
        "the dropdown is open before the background click"
    );

    let background = vcx
        .debug_bounds("outside-click-background-target")
        .expect("background control must render beneath the dropdown");
    vcx.simulate_click(background.center(), Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());

    assert_eq!(
        *activations.borrow(),
        1,
        "GPUI dismisses the dropdown and forwards the same click to its background target"
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

struct ToggleDropdownHarness {
    builds: Rc<RefCell<usize>>,
    toggles: Rc<RefCell<Vec<usize>>>,
    menu_focus: Rc<RefCell<Option<FocusHandle>>>,
    dropdown_state_probe: Rc<RefCell<Option<DropdownStateHandle>>>,
}

type DropdownStateHandle = Rc<RefCell<Option<Entity<super::DropdownMenuState>>>>;

struct OutsideClickHarness {
    activations: Rc<RefCell<usize>>,
}

impl Render for OutsideClickHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let activations = self.activations.clone();
        let palette = Theme::dark().palette();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(DropdownMenu::new(
                "outside-click-dropdown",
                div()
                    .id("outside-click-dropdown-trigger")
                    .debug_selector(|| "outside-click-dropdown-trigger".into())
                    .w(px(120.0))
                    .h(px(32.0)),
                palette,
                |state, _| state,
            ))
            .child(div().h(px(600.0)))
            .child(
                div()
                    .id("outside-click-background-target")
                    .debug_selector(|| "outside-click-background-target".into())
                    .w(px(120.0))
                    .h(px(40.0))
                    .on_click(move |_event, _window, _cx| {
                        *activations.borrow_mut() += 1;
                    }),
            )
    }
}

impl Render for ToggleDropdownHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::dark().palette();
        let builds = self.builds.clone();
        let toggles = self.toggles.clone();
        let menu_focus = self.menu_focus.clone();
        let dropdown = DropdownMenu::new(
            "dropdown-toggle",
            div()
                .id("dropdown-toggle-trigger")
                .debug_selector(|| "dropdown-toggle-trigger".into())
                .focusable()
                .tab_stop(true)
                .w_32()
                .h_8(),
            palette,
            move |_state, cx| {
                *builds.borrow_mut() += 1;
                let first_toggles = toggles.clone();
                let second_toggles = toggles.clone();
                let state = PopupMenuState::new(
                    vec![
                        MenuEntry::Item(
                            MenuItem::new("first", move |_, _| {
                                first_toggles.borrow_mut().push(0);
                            })
                            .checked(false)
                            .checkable(true),
                        ),
                        MenuEntry::Item(
                            MenuItem::new("second", move |_, _| {
                                second_toggles.borrow_mut().push(1);
                            })
                            .checked(false)
                            .checkable(true),
                        ),
                    ],
                    cx,
                );
                *menu_focus.borrow_mut() = Some(state.focus_handle(cx));
                state
            },
        );
        *self.dropdown_state_probe.borrow_mut() = Some(dropdown.anchor_state.clone());
        div().size_full().child(dropdown)
    }
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
