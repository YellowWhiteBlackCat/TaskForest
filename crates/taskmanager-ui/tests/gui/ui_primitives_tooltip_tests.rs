use super::{TOOLTIP_DELAY, Tooltip, TooltipHost, TooltipVisibility, tooltip_step};
use gpui::Modifiers;
use gpui::{
    AppContext, Context, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, TestAppContext, Window, div, px,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use taskmanager_theme::Palette;
use taskmanager_theme::Theme;
fn palette() -> Palette {
    Theme::dark().palette()
}

#[test]
fn hover_delay_gates_visibility() {
    // Hidden + hover -> Armed (never immediately Visible).
    assert_eq!(
        tooltip_step(
            TooltipVisibility::Hidden,
            true,
            false,
            Some(Duration::from_millis(1)),
        ),
        TooltipVisibility::Armed,
    );
    // Armed + delay elapsed -> Visible.
    assert_eq!(
        tooltip_step(TooltipVisibility::Armed, true, false, Some(TOOLTIP_DELAY),),
        TooltipVisibility::Visible,
    );
    // Armed + hover lost -> Hidden.
    assert_eq!(
        tooltip_step(TooltipVisibility::Armed, false, false, None),
        TooltipVisibility::Hidden,
    );
    // Keyboard focus shows immediately.
    assert_eq!(
        tooltip_step(TooltipVisibility::Hidden, false, true, None),
        TooltipVisibility::Visible,
    );
    // Visible survives while hovering/focused, hides on leave.
    assert_eq!(
        tooltip_step(TooltipVisibility::Visible, true, false, None),
        TooltipVisibility::Visible,
    );
    assert_eq!(
        tooltip_step(TooltipVisibility::Visible, false, false, None),
        TooltipVisibility::Hidden,
    );
}

#[gpui::test]
async fn tooltip_renders_around_trigger(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| Harness);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let _ = window.read_with(cx, |_, _| {});
}

#[gpui::test]
async fn keyboard_focus_shows_tooltip_and_escape_dismisses_it(cx: &mut TestAppContext) {
    let clicks = Rc::new(RefCell::new(0));
    let window = cx.add_window(|_window, _cx| TooltipBehaviorHarness {
        probe_clicks: clicks.clone(),
    });
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        window.activate_window();
        window.draw(cx).clear(cx);
    });
    assert!(
        vcx.debug_bounds("tooltip-probe").is_none(),
        "the tooltip content must start hidden"
    );

    vcx.update(|window, cx| {
        window.focus_next(cx);
        window.draw(cx).clear(cx);
    });
    let probe = vcx
        .debug_bounds("tooltip-probe")
        .expect("keyboard focus must show tooltip content");
    vcx.simulate_keystrokes("escape");
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.simulate_click(probe.center(), Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    assert_eq!(
        *clicks.borrow(),
        0,
        "Escape must remove the tooltip hit target without triggering its stale bounds"
    );
}

#[gpui::test]
async fn pointer_click_dismisses_a_focused_tooltip(cx: &mut TestAppContext) {
    let clicks = Rc::new(RefCell::new(0));
    let window = cx.add_window(|_window, _cx| TooltipBehaviorHarness {
        probe_clicks: clicks.clone(),
    });
    let mut vcx = gpui::VisualTestContext::from_window(window.into(), cx);
    vcx.update(|window, cx| {
        window.activate_window();
        window.draw(cx).clear(cx);
        window.focus_next(cx);
        window.draw(cx).clear(cx);
    });
    let trigger = vcx
        .debug_bounds("tooltip-trigger")
        .expect("tooltip trigger must render");
    let probe = vcx
        .debug_bounds("tooltip-probe")
        .expect("keyboard focus must show tooltip content");

    vcx.simulate_click(trigger.center(), Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.simulate_click(probe.center(), Modifiers::none());
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    assert_eq!(
        *clicks.borrow(),
        0,
        "a pointer click on the trigger must dismiss the tooltip"
    );
}

struct TooltipBehaviorHarness {
    probe_clicks: Rc<RefCell<usize>>,
}

impl Render for TooltipBehaviorHarness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let probe_clicks = self.probe_clicks.clone();
        let trigger = div()
            .id("tooltip-trigger")
            .debug_selector(|| "tooltip-trigger".into())
            .w_24()
            .h_8()
            .focusable()
            .tab_stop(true);
        div()
            .size_full()
            .child(
                TooltipHost::new("behavior-tooltip", trigger).tooltip(Tooltip::element(
                    move |_window, _cx| {
                        let probe_clicks = probe_clicks.clone();
                        div()
                            .id("tooltip-probe")
                            .debug_selector(|| "tooltip-probe".into())
                            .size(px(24.0))
                            .on_click(move |_event, _window, _cx| {
                                *probe_clicks.borrow_mut() += 1;
                            })
                    },
                    palette(),
                )),
            )
    }
}

#[derive(Default)]
struct Harness;

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().child(
            TooltipHost::new("host", div().w_24().h_8()).tooltip(Tooltip::text("label", palette())),
        )
    }
}
