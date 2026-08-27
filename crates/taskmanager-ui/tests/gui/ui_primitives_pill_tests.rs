use super::{Pill, PillState};
use gpui::{AppContext, Context, IntoElement, ParentElement, Render, TestAppContext, Window, div};
use taskmanager_theme::Theme;
#[gpui::test]
async fn pill_renders_both_states(cx: &mut TestAppContext) {
    let window = cx.add_window(|_window, _cx| Harness);
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
        .unwrap();
    let _ = window.read_with(cx, |_, _| {});
}

#[derive(Default)]
struct Harness;

impl Render for Harness {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::dark().palette();
        div()
            .child(Pill::new("all", PillState::Active, palette))
            .child(Pill::new("apps", PillState::Idle, palette))
    }
}
