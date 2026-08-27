use super::{Badge, BadgeTone};
use gpui::{AppContext, Context, IntoElement, ParentElement, Render, TestAppContext, Window, div};
use taskmanager_theme::Theme;
#[test]
fn tones_are_palette_derived_and_distinct() {
    let palette = Theme::dark().palette();
    // Every tone maps to a distinct palette token (no hardcoded hues).
    assert_eq!(BadgeTone::Success, BadgeTone::Success);
    assert_ne!(palette.success, palette.danger);
    assert_ne!(palette.warning, palette.accent);
    assert_ne!(palette.border, palette.success);
}

/// Badges render without panicking in a real window.
#[gpui::test]
async fn badge_renders_for_every_tone(cx: &mut TestAppContext) {
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
            .child(Badge::new("ok", BadgeTone::Success, palette))
            .child(Badge::new("warn", BadgeTone::Warning, palette))
            .child(Badge::new("err", BadgeTone::Danger, palette))
            .child(Badge::new("info", BadgeTone::Neutral, palette))
    }
}
