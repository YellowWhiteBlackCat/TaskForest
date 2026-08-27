use super::ContextMenuExt;
use crate::overlays::popup::{MenuEntry, MenuItem, PopupMenuState};
use gpui::{
    AppContext, Context, InteractiveElement, IntoElement, Modifiers, MouseButton, ParentElement,
    Render, Styled, TestAppContext, Window, div, point, px,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::Theme;
#[gpui::test]
async fn context_menu_renders(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| Harness);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let _ = window.read_with(cx, |_, _| {});
}

/// Regression lock (listener accumulation): the right-click listener is
/// attached to the wrapped element, which is rebuilt every frame — N
/// draws must never yield N listeners. One right-click opens the menu
/// exactly once (the old per-paint `window.on_mouse_event` registration
/// fired the builder once per drawn frame).
#[gpui::test]
async fn right_click_builds_menu_exactly_once_after_many_draws(cx: &mut TestAppContext) {
    let builds = Rc::new(RefCell::new(0));
    let window = cx.add_window(|_window, _cx| ContextMenuHarness {
        builds: builds.clone(),
    });

    // Many draws (request_layout every frame): the old implementation
    // registered one window-level listener per paint.
    for _ in 0..4 {
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .unwrap();
    }

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    let position = point(px(120.0), px(120.0));
    vcx.simulate_mouse_down(position, MouseButton::Right, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        *builds.borrow(),
        1,
        "one right-click must build the menu exactly once, even after many draws"
    );
    let popup = vcx
        .debug_bounds("tm-popup")
        .expect("the popup must render once the menu is open");
    // The popup anchors at the right-click position; the entrance
    // animation may still be rising (up to 4px) on the first drawn frame.
    assert_eq!(popup.left(), px(120.0), "anchored horizontally");
    assert!(
        popup.top() >= px(120.0) && popup.top() <= px(124.0),
        "the popup rises into place from the right-click position, got {:?}",
        popup.top()
    );

    // A second right-click opens the menu once more — still exactly one
    // build per click (no listener pile-up).
    vcx.simulate_mouse_down(position, MouseButton::Right, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        *builds.borrow(),
        2,
        "each right-click must build the menu exactly once"
    );

    // Left clicks are not right clicks: the menu must not reopen.
    vcx.simulate_click(point(px(300.0), px(300.0)), Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        *builds.borrow(),
        2,
        "a left click must not open the context menu"
    );
}

/// Regression lock (dismiss wiring): the host must subscribe to the menu
/// entity's `DismissEvent`. Before, no subscriber cleared `open`, so the
/// menu rendered forever — outside click, Escape and item activation all
/// dismissed the entity while the host kept drawing it.
///
/// Closure is asserted behaviorally (activations must not run once the
/// menu closed) because gpui 0.2.2's `debug_bounds` map is append-only
/// across frames — a stale entry does not prove the popup still paints.
#[gpui::test]
async fn context_menu_closes_on_outside_click_escape_and_item_activation(cx: &mut TestAppContext) {
    cx.update(super::super::popup::init);
    let activations = Rc::new(RefCell::new(0));
    let window = cx.add_window(|_window, _cx| ContextMenuCloseHarness {
        activations: activations.clone(),
    });

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| window.draw(cx).clear());
    // The trigger sits at the window origin (w_64 × h_16): its center.
    let open_at = point(px(128.0), px(32.0));

    // Right-click opens the menu at the click position.
    vcx.simulate_mouse_down(open_at, MouseButton::Right, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    let popup = vcx.debug_bounds("tm-popup").expect("menu must open");
    // Item 0 center: body py(SPACE_4) + half of the 26px row; the
    // entrance animation rises up to 4px, which stays inside the row.
    let item_center = point(popup.left() + px(40.0), popup.top() + px(17.0));

    // Outside click closes: the item must no longer be clickable.
    vcx.simulate_click(point(px(400.0), px(300.0)), Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    vcx.simulate_click(item_center, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        *activations.borrow(),
        0,
        "outside click must close the menu (the item is no longer clickable)"
    );

    // Escape closes (the open path focused the menu).
    vcx.simulate_mouse_down(open_at, MouseButton::Right, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    vcx.simulate_keystrokes("escape");
    vcx.update(|window, cx| window.draw(cx).clear());
    vcx.simulate_click(item_center, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        *activations.borrow(),
        0,
        "Escape must close the menu (the item is no longer clickable)"
    );

    // Item activation runs exactly once and closes the menu.
    vcx.simulate_mouse_down(open_at, MouseButton::Right, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    vcx.simulate_click(item_center, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        *activations.borrow(),
        1,
        "the activated item must run exactly once"
    );
    // The same spot must not re-activate: activation closed the menu.
    vcx.simulate_click(item_center, Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        *activations.borrow(),
        1,
        "item activation must close the menu (no re-activation)"
    );
}

#[derive(Default)]
struct Harness;

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::dark().palette();
        div()
            .id("trigger")
            .w_32()
            .h_8()
            .context_menu("ctx", palette, |state, _cx| state)
    }
}

struct ContextMenuHarness {
    builds: Rc<RefCell<usize>>,
}
impl Render for ContextMenuHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::dark().palette();
        let builds = self.builds.clone();
        div()
            .size_full()
            .child(div().id("ctx-trigger").size_full().context_menu(
                "ctx",
                palette,
                move |state, _cx| {
                    *builds.borrow_mut() += 1;
                    state
                },
            ))
    }
}

/// Harness for the dismiss-wiring lock: a right-clickable trigger whose
/// single menu item counts its activations.
struct ContextMenuCloseHarness {
    activations: Rc<RefCell<usize>>,
}

impl Render for ContextMenuCloseHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::dark().palette();
        let activations = self.activations.clone();
        div()
            .id("trigger")
            .w_64()
            .h_16()
            .context_menu("ctx-close", palette, move |_state, cx| {
                let activations = activations.clone();
                PopupMenuState::new(
                    vec![MenuEntry::Item(MenuItem::new("act", move |_, _| {
                        *activations.borrow_mut() += 1;
                    }))],
                    cx,
                )
            })
    }
}
