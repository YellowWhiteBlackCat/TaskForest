use super::{LayerEntry, LayerId, LayerStack, ModalSpec, PaletteScrim};
use crate::focus::ModalEscTarget;
use gpui::{
    App, AppContext, Context, Entity, InteractiveElement, IntoElement, Modifiers, MouseButton,
    ParentElement, Render, Styled, TestAppContext, Window, div, point, px,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_theme::Theme;
#[test]
fn layer_ids_mint_and_wrap() {
    let mut next = u64::MAX - 1;
    assert_eq!(LayerId::mint(&mut next).value(), u64::MAX - 1);
    assert_eq!(LayerId::mint(&mut next).value(), u64::MAX);
    // Wraps to 1 (never reuses 0).
    assert_eq!(LayerId::mint(&mut next).value(), 1);
}

fn dummy_entry(cx: &mut App, id: u64, is_modal: bool) -> LayerEntry {
    LayerEntry {
        id: LayerId(id),
        is_modal,
        mask: None,
        mask_closable: false,
        focus_handle: cx.focus_handle(),
        content: Rc::new(|_, _, _| div().into_any_element()),
    }
}

#[gpui::test]
async fn esc_target_prioritizes_top_layer(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mut stack = LayerStack::new();
        assert_eq!(stack.esc_target(), ModalEscTarget::Window);
        stack.layers.push(dummy_entry(cx, 1, false));
        assert_eq!(stack.esc_target(), ModalEscTarget::Popup);
        stack.layers.push(dummy_entry(cx, 2, true));
        assert_eq!(stack.esc_target(), ModalEscTarget::Dialog);
    });
}

#[gpui::test]
async fn layer_ix_and_close_any_layer_by_id(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mut stack = LayerStack::new();
        stack.layers.push(dummy_entry(cx, 1, false));
        stack.layers.push(dummy_entry(cx, 2, true));
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.layer_ix(LayerId(1)), Some(0));
        assert_eq!(stack.layer_ix(LayerId(2)), Some(1));
        assert_eq!(stack.layer_ix(LayerId(99)), None);
    });
}

/// End-to-end: push a modal through the real render path, then close it
/// via the backfill's close closure; the stack empties and the ESC
/// target returns to the window.
#[gpui::test]
async fn push_close_via_backfill(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, cx| {
        let stack = cx.new(|_| LayerStack::new());
        Harness { stack }
    });
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let id = window
        .update(cx, |harness, window, cx| {
            harness.stack.update(cx, |stack, cx| {
                stack.push_modal(
                    ModalSpec {
                        mask: Some(PaletteScrim::new(Theme::dark().palette(), 0.5)),
                        mask_closable: true,
                        keyboard: true,
                        content: Rc::new(|_backfill, _, _| div().child("modal").into_any_element()),
                    },
                    window,
                    cx,
                )
            })
        })
        .unwrap();

    assert_eq!(
        window
            .read_with(cx, |harness, app| harness.stack.read(app).len())
            .unwrap(),
        1
    );
    assert_eq!(
        window
            .read_with(cx, |harness, app| harness.stack.read(app).esc_target())
            .unwrap(),
        ModalEscTarget::Dialog
    );

    // The backfill close closure empties the stack (host wiring).
    let _ = window.update(cx, |harness, window, cx| {
        harness
            .stack
            .update(cx, |stack, cx| stack.close(id, window, cx))
    });
    assert_eq!(
        window
            .read_with(cx, |harness, app| harness.stack.read(app).len())
            .unwrap(),
        0
    );
    assert_eq!(
        window
            .read_with(cx, |harness, app| harness.stack.read(app).esc_target())
            .unwrap(),
        ModalEscTarget::Window
    );
}

/// Regression: the modal layer mask must cover the whole window. gpui
/// elements default to `position: relative`, so a plain auto-height stack
/// root made `.absolute()` layer children resolve against a ~0-height box
/// — dialogs collapsed and never painted (capture evidence: no scrim, no
/// panel). The absolute full-size root pins the window as the containing
/// block; this test locks the mask geometry.
#[gpui::test]
async fn modal_mask_covers_the_window_and_panel_stays_inside(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, cx| {
        let stack = cx.new(|_| LayerStack::new());
        Harness { stack }
    });
    window
        .update(cx, |harness, window, cx| {
            harness.stack.update(cx, |stack, cx| {
                stack.push_modal(
                    ModalSpec {
                        mask: Some(PaletteScrim::new(Theme::dark().palette(), 0.5)),
                        mask_closable: true,
                        keyboard: true,
                        content: Rc::new(|_backfill, _, _| {
                            div().w(px(300.0)).h(px(200.0)).into_any_element()
                        }),
                    },
                    window,
                    cx,
                )
            })
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mask = {
        let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
        vcx.debug_bounds("tm-layer-mask:0")
            .expect("modal mask must render with a debug selector")
    };
    let window_bounds = cx
        .update_window(window.into(), |_, window, _| window.bounds())
        .unwrap();
    assert_eq!(
        mask, window_bounds,
        "modal mask must cover the window (regression: auto-height stack root)"
    );
}

struct Harness {
    stack: Entity<LayerStack>,
}

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Mirror the app's window root (RootView render): full-size so the
        // absolute stack root resolves its containing block to the window.
        div().size_full().child(self.stack.clone())
    }
}

/// 附录 A-22 lock: the modal mask closes on LEFT click only when
/// `mask_closable`. Observation (product code untouched): the mask's
/// `on_any_mouse_down` calls `stop_propagation()` for every button, so a
/// right-click is swallowed too — the "intercept only Left" intent is
/// only partially implemented (right-click never closes, but it also
/// never reaches the page underneath). The test locks the shipped
/// behavior and records the observation.
#[gpui::test]
async fn mask_right_click_does_not_close_but_left_click_does(cx: &mut TestAppContext) {
    let page_clicks = Rc::new(RefCell::new((0usize, 0usize)));
    let window = cx.add_window(|_window, cx| {
        let stack = cx.new(|_| LayerStack::new());
        MaskHarness {
            stack,
            page_clicks: page_clicks.clone(),
        }
    });
    window
        .update(cx, |harness, window, cx| {
            harness.stack.update(cx, |stack, cx| {
                stack.push_modal(
                    ModalSpec {
                        mask: Some(PaletteScrim::new(Theme::dark().palette(), 0.5)),
                        mask_closable: true,
                        keyboard: true,
                        content: Rc::new(|_backfill, _, _| {
                            div().w(px(300.0)).h(px(200.0)).into_any_element()
                        }),
                    },
                    window,
                    cx,
                )
            })
        })
        .unwrap();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();

    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    // Bottom-right of the 1920x1080 test window: on the mask, clear of
    // the 300x200 panel.
    let on_mask = point(px(1500.0), px(900.0));

    // Right-click on the mask: must NOT close the modal...
    vcx.simulate_mouse_down(on_mask, MouseButton::Right, Modifiers::none());
    vcx.simulate_mouse_up(on_mask, MouseButton::Right, Modifiers::none());
    drop(vcx);
    let len = window
        .read_with(cx, |harness, app| harness.stack.read(app).len())
        .unwrap();
    assert_eq!(len, 1, "right-click on the mask must not close the modal");
    let (left_clicks, right_clicks) = *page_clicks.borrow();
    assert_eq!(
        (left_clicks, right_clicks),
        (0, 0),
        "observation A-22: the right-click is stop_propagation'd and never reaches the page"
    );

    // ...but the left click closes it (mask_closable).
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.simulate_click(on_mask, Modifiers::none());
    drop(vcx);
    let len = window
        .read_with(cx, |harness, app| harness.stack.read(app).len())
        .unwrap();
    assert_eq!(len, 0, "left-click on the mask must close the modal");
    let (left_clicks, right_clicks) = *page_clicks.borrow();
    assert_eq!(
        (left_clicks, right_clicks),
        (0, 0),
        "the left click is also swallowed (closing replaces page interaction)"
    );
}

struct MaskHarness {
    stack: Entity<LayerStack>,
    page_clicks: Rc<RefCell<(usize, usize)>>,
}

impl Render for MaskHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let left = self.page_clicks.clone();
        let right = self.page_clicks.clone();
        div()
            .size_full()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                left.borrow_mut().0 += 1;
                let _ = cx;
            })
            .on_mouse_down(MouseButton::Right, move |_, _, cx| {
                right.borrow_mut().1 += 1;
                let _ = cx;
            })
            .child(self.stack.clone())
    }
}
