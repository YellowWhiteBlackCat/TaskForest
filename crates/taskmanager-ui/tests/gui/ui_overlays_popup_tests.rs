use super::{MenuEntry, MenuItem, PopupMenuState, SelectionDirection};
use gpui::prelude::FluentBuilder;
use gpui::{
    AppContext, Context, DismissEvent, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, Modifiers, MouseButton, ParentElement, Render, Styled, TestAppContext, Window,
    WindowHandle, div, point, px,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::Theme;
/// Selection skips disabled items and separators, wrapping around.
#[gpui::test]
async fn selection_skips_disabled_and_wraps(cx: &mut TestAppContext) {
    let menu = cx.new(|cx| {
        PopupMenuState::new(
            vec![
                MenuEntry::Item(MenuItem::new("one", |_, _| {})),
                MenuEntry::Separator,
                MenuEntry::Item(MenuItem::new("two", |_, _| {}).disabled(true)),
                MenuEntry::Item(MenuItem::new("three", |_, _| {})),
            ],
            cx,
        )
    });
    menu.update(cx, |menu, cx| {
        menu.move_selection(SelectionDirection::Down, cx);
        assert_eq!(menu.selected(), Some(0));
        // Down from 0 must skip the separator + disabled item.
        menu.move_selection(SelectionDirection::Down, cx);
        assert_eq!(menu.selected(), Some(3));
        // Wraps back to the first selectable.
        menu.move_selection(SelectionDirection::Down, cx);
        assert_eq!(menu.selected(), Some(0));
        // Up wraps to the last selectable.
        menu.move_selection(SelectionDirection::Up, cx);
        assert_eq!(menu.selected(), Some(3));
    });
}

/// An all-separator menu has no selectable items.
#[gpui::test]
async fn empty_menu_has_no_selectable_items(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let menu = PopupMenuState::new(vec![MenuEntry::Separator], cx);
        assert!(menu.is_empty());
    });
}

/// A host window with an optional popup menu and a focusable trigger.
struct PopupHarness {
    trigger_focus: FocusHandle,
    menu: Option<Entity<PopupMenuState>>,
}

impl Render for PopupHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let trigger_focus = self.trigger_focus.clone().tab_stop(true);
        let trigger = div().id("trigger").w_32().h_8().track_focus(&trigger_focus);
        div()
            .size_full()
            .child(trigger)
            .when_some(self.menu.clone(), |el, menu| el.child(menu))
    }
}

/// Open the menu in the harness; the host closes it on `DismissEvent`
/// (the same wiring as the table's context menu). Returns the menu
/// entity, a dismiss counter, and the host subscription (the caller
/// must keep it alive — dropping it silently disconnects the host).
fn open_with_host(
    cx: &mut TestAppContext,
    window: WindowHandle<PopupHarness>,
    items: Vec<MenuEntry>,
) -> (
    Entity<PopupMenuState>,
    Rc<RefCell<usize>>,
    gpui::Subscription,
) {
    let trigger = window
        .read_with(cx, |harness, _| harness.trigger_focus.clone())
        .unwrap();
    let menu = window
        .update(cx, |_harness, window, cx| {
            let mut state = PopupMenuState::new(items, cx);
            state.set_action_context(trigger);
            state.mount(
                Theme::dark().palette(),
                point(px(200.0), px(200.0)),
                window,
                cx,
            )
        })
        .unwrap();
    let dismisses = Rc::new(RefCell::new(0));
    let sink = dismisses.clone();
    let menu_for_subscription = menu.clone();
    let host_subscription = cx.update(move |cx| {
        cx.subscribe(
            &menu_for_subscription,
            move |_menu_entity, _: &DismissEvent, cx| {
                *sink.borrow_mut() += 1;
                let _ = window.update(cx, |harness, _window, cx| {
                    harness.menu = None;
                    cx.notify();
                });
            },
        )
    });
    window
        .update(cx, |harness, _window, cx| {
            harness.menu = Some(menu.clone());
            cx.notify();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    (menu, dismisses, host_subscription)
}

/// 附录 A-6 lock: a mouse-down outside the popup dismisses it and the
/// host closes the menu; a mouse-down inside the popup does not (gc only
/// checked the immediate parent menu's bounds).
#[gpui::test]
async fn outside_click_dismisses_menu_but_inside_click_does_not(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, cx| PopupHarness {
        trigger_focus: cx.focus_handle().tab_stop(true),
        menu: None,
    });
    let (menu, dismisses, _host_subscription) =
        open_with_host(cx, window, vec![MenuEntry::Label("header".into())]);

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    // Click inside the popup (on the non-interactive label): no dismiss.
    let popup = vcx.debug_bounds("tm-popup").expect("popup must render");
    vcx.simulate_click(popup.center(), Modifiers::none());
    drop(vcx);
    assert_eq!(
        *dismisses.borrow(),
        0,
        "a click inside the popup must not dismiss it"
    );

    // Click far outside: the popup dismisses, the host closes the menu,
    // and the trigger focus is restored.
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_click(point(px(500.0), px(400.0)), Modifiers::none());
    drop(vcx);
    assert_eq!(
        *dismisses.borrow(),
        1,
        "a click outside the popup must dismiss it"
    );
    let (menu_open, trigger_focused) = window
        .update(cx, |harness, window, _| {
            (
                harness.menu.is_some(),
                harness.trigger_focus.is_focused(window),
            )
        })
        .unwrap();
    assert!(!menu_open, "the host must close the menu on DismissEvent");
    assert!(
        trigger_focused,
        "dismiss must restore the trigger focus (absorption 3.3-B)"
    );
    let _ = menu;
}

/// 附录 A-16 lock: `selected` is the "most recently activated item", not
/// the hovered item — leaving an item keeps its selection until another
/// item is hovered (gc cleared it on hover-out).
#[gpui::test]
async fn hover_out_keeps_last_selected_item(cx: &mut TestAppContext) {
    let menu = cx.new(|cx| {
        let mut state = PopupMenuState::new(
            vec![
                MenuEntry::Item(MenuItem::new("one", |_, _| {})),
                MenuEntry::Item(MenuItem::new("two", |_, _| {})),
            ],
            cx,
        );
        state.present(Theme::dark().palette(), point(px(200.0), px(200.0)));
        state
    });
    let window = cx.add_window(|_window, _cx| PopupMenuHarness { menu: menu.clone() });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    let popup = vcx.debug_bounds("tm-popup").expect("popup must render");
    // Items are 26px tall inside the body's 4px top padding.
    let item_center = |ix: usize| {
        point(
            popup.left() + px(40.0),
            popup.top() + px(17.0) + px(26.0 * ix as f32),
        )
    };

    // Hover item 0: selected.
    vcx.simulate_mouse_move(item_center(0), None::<MouseButton>, Modifiers::none());
    assert_eq!(menu.read_with(&vcx, |m, _| m.selected()), Some(0));

    // Move entirely out: the selection must survive (A-16).
    vcx.simulate_mouse_move(
        point(px(30.0), px(30.0)),
        None::<MouseButton>,
        Modifiers::none(),
    );
    assert_eq!(
        menu.read_with(&vcx, |m, _| m.selected()),
        Some(0),
        "hover-out must not clear the selected item"
    );

    // Hover item 1: the selection moves.
    vcx.simulate_mouse_move(item_center(1), None::<MouseButton>, Modifiers::none());
    assert_eq!(menu.read_with(&vcx, |m, _| m.selected()), Some(1));
}

/// 附录 A-17 lock: Escape is routed through the focused popup's key
/// context (the ESC chain: popup wins over dialog/window). Dispatching
/// Escape to the focused popup dismisses it and restores the trigger
/// focus.
#[gpui::test]
async fn escape_closes_popup_and_restores_trigger_focus(cx: &mut TestAppContext) {
    cx.update(super::init);
    let window = cx.add_window(|_window, cx| PopupHarness {
        trigger_focus: cx.focus_handle(),
        menu: None,
    });
    let (menu, dismisses, _host_subscription) = open_with_host(
        cx,
        window,
        vec![MenuEntry::Item(MenuItem::new("one", |_, _| {}))],
    );

    // Open actions own the focus: focus the popup, then press Escape.
    let popup_focus = menu.read_with(cx, |m, cx| m.focus_handle(cx));
    window
        .update(cx, |_, window, cx| {
            window.activate_window();
            popup_focus.focus(window);
            cx.notify();
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_keystrokes("escape");
    drop(vcx);

    assert_eq!(
        *dismisses.borrow(),
        1,
        "Escape must dismiss the focused popup"
    );
    let (menu_open, trigger_focused) = window
        .update(cx, |harness, window, _| {
            (
                harness.menu.is_some(),
                harness.trigger_focus.is_focused(window),
            )
        })
        .unwrap();
    assert!(!menu_open, "the host must close the menu on Escape");
    assert!(trigger_focused, "Escape must restore the trigger focus");
}

/// A window that always renders one popup menu at (200, 200).
struct PopupMenuHarness {
    menu: Entity<PopupMenuState>,
}

impl Render for PopupMenuHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.menu.clone())
    }
}
