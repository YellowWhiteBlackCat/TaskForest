use super::{Divider, DividerOrientation};
use gpui::{AppContext, Context, IntoElement, ParentElement, Render, TestAppContext, Window, div};
use taskmanager_theme::Theme;
#[gpui::test]
async fn dividers_render_in_both_orientations(cx: &mut TestAppContext) {
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
            .child(Divider::new(DividerOrientation::Vertical, palette))
            .child(Divider::new(DividerOrientation::Horizontal, palette))
    }
}
