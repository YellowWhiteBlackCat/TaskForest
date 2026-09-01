//! Shared empty, unavailable, and other state-panel visual grammar.

use crate::icons_binding::icon;
use gpui::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px,
};
use taskmanager_theme::{Color, Palette, tokens};
use taskmanager_ui_contract::IconId;

/// Centered state content with a quiet icon tile and optional detail/action.
///
/// State meaning and localized copy stay with the caller. This component only
/// guarantees the same geometry and visual hierarchy for empty, unavailable,
/// partial, and recovery states.
#[derive(IntoElement)]
pub struct StatePanel {
    icon: IconId,
    title: SharedString,
    detail: Option<SharedString>,
    action: Option<AnyElement>,
    tone: Color,
    palette: Palette,
}

impl StatePanel {
    /// Build a neutral state panel using the palette accent as its tone.
    pub fn new(icon: IconId, title: impl Into<SharedString>, palette: Palette) -> Self {
        Self {
            icon,
            title: title.into(),
            detail: None,
            action: None,
            tone: palette.accent,
            palette,
        }
    }

    /// Add a secondary explanatory line.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Select the semantic tone for the icon tile.
    #[must_use]
    pub fn tone(mut self, tone: Color) -> Self {
        self.tone = tone;
        self
    }

    /// Add a recovery/action affordance below the copy.
    #[must_use]
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    /// Build the concrete `Div` for callers that need to add page-local
    /// children after the shared state content.
    #[must_use]
    pub fn render(self) -> gpui::Div {
        let mut content = div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(crate::theme_binding::definite_length(tokens::SPACE_8))
            .child(
                div()
                    .size(px(42.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(crate::theme_binding::fill(self.tone.with_alpha(0.12)))
                    .border_1()
                    .border_color(crate::theme_binding::hsla(self.tone.with_alpha(0.30)))
                    .child(
                        icon(self.icon)
                            .size(px(22.0))
                            .text_color(crate::theme_binding::hsla(self.tone)),
                    ),
            )
            .child(
                div()
                    .max_w(px(360.0))
                    .text_size(crate::theme_binding::font_size(tokens::FONT_13))
                    .text_color(crate::theme_binding::hsla(self.palette.fg_muted))
                    .child(self.title),
            );
        if let Some(detail) = self.detail {
            content = content.child(
                div()
                    .max_w(px(360.0))
                    .text_size(crate::theme_binding::font_size(tokens::FONT_12))
                    .text_color(crate::theme_binding::hsla(self.palette.fg_muted))
                    .child(detail),
            );
        }
        if let Some(action) = self.action {
            content = content.child(action);
        }
        content
    }
}

impl RenderOnce for StatePanel {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.render()
    }
}
