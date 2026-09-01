//! Status badge (stateless visual primitive).

use gpui::{App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px};
use taskmanager_theme::Palette;

use taskmanager_theme::color::on_accent;
use taskmanager_theme::tokens;

/// Badge tones; each maps to a palette semantic color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTone {
    /// Neutral: `palette.border`-based.
    Neutral,
    /// `palette.success`
    Success,
    /// `palette.warning`
    Warning,
    /// `palette.danger`
    Danger,
    /// `palette.accent`
    Accent,
}

/// A small status pill with a colored fill derived from [`Palette`].
#[derive(IntoElement)]
pub struct Badge {
    text: SharedString,
    tone: BadgeTone,
    palette: Palette,
}

impl Badge {
    /// Build a badge with the given tone.
    pub fn new(text: impl Into<SharedString>, tone: BadgeTone, palette: Palette) -> Self {
        Self {
            text: text.into(),
            tone,
            palette,
        }
    }
}

impl RenderOnce for Badge {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (fill, foreground) = match self.tone {
            BadgeTone::Neutral => (self.palette.surface, self.palette.fg_muted),
            BadgeTone::Success => (self.palette.success, on_accent(self.palette.success)),
            BadgeTone::Warning => (self.palette.warning, on_accent(self.palette.warning)),
            BadgeTone::Danger => (self.palette.danger, on_accent(self.palette.danger)),
            BadgeTone::Accent => (self.palette.accent, on_accent(self.palette.accent)),
        };
        div()
            .flex()
            .items_center()
            .px(crate::theme_binding::definite_length(tokens::SPACE_8))
            .h(px(20.0))
            .rounded_full()
            .bg(crate::theme_binding::fill(fill))
            .text_color(crate::theme_binding::hsla(foreground))
            .text_xs()
            .font_weight(crate::theme_binding::font_weight(
                tokens::FONT_WEIGHT_MEDIUM,
            ))
            .child(self.text)
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_badge_tests.rs"]
mod tests;
