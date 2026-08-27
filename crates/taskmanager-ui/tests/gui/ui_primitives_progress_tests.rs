use super::ProgressBar;
use gpui::{AppContext, Context, IntoElement, ParentElement, Render, TestAppContext, Window, div};
use taskmanager_theme::Theme;
#[test]
fn determinate_value_is_clamped() {
    let p = Theme::dark().palette();
    assert_eq!(ProgressBar::new(-0.5, p).value(), Some(0.0));
    assert_eq!(ProgressBar::new(1.5, p).value(), Some(1.0));
    assert_eq!(ProgressBar::new(0.42, p).value(), Some(0.42));
    assert_eq!(ProgressBar::indeterminate(p).value(), None);
}

#[gpui::test]
async fn progress_renders_both_modes(cx: &mut TestAppContext) {
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
            .child(ProgressBar::new(0.6, palette))
            .child(ProgressBar::indeterminate(palette))
    }
}
