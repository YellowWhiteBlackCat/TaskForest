use super::{TOOLTIP_DELAY, Tooltip, TooltipHost, TooltipVisibility, tooltip_step};
use gpui::{
    AppContext, Context, IntoElement, ParentElement, Render, Styled, TestAppContext, Window, div,
};
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
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let _ = window.read_with(cx, |_, _| {});
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
