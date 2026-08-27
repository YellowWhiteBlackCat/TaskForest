//! Segmented-control pill / filter chip (stateless visual primitive,
//! absorbing `crates/taskmanager-gpui/src/gpui_app/elements.rs::pill`).

use gpui::{App, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px};
use taskmanager_theme::Palette;

use crate::primitives::motion::{hover_bg_transition, hover_state_key};
use crate::styled::on_accent;
use taskmanager_theme::tokens;

/// Pill states; the accent fill family is palette-derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillState {
    /// Inactive: subtle surface fill.
    Idle,
    /// Active: accent fill + `on_accent` text.
    Active,
}

/// A filter pill / segmented segment.
#[derive(IntoElement)]
pub struct Pill {
    text: SharedString,
    state: PillState,
    palette: Palette,
    radius: f32,
    hovered: bool,
}

impl Pill {
    /// Build a pill.
    pub fn new(text: impl Into<SharedString>, state: PillState, palette: Palette) -> Self {
        Self {
            text: text.into(),
            state,
            palette,
            radius: 9999.0,
            hovered: false,
        }
    }

    /// Corner radius override (default fully rounded).
    #[must_use]
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Idle-segment hover overlay: swaps the surface fill for the palette's
    /// translucent `hover` tint so an inactive segment gives a visible hover
    /// affordance (the documented behavior the wrapper in elements.rs forwards).
    #[must_use]
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }
}

impl RenderOnce for Pill {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        match self.state {
            // Idle pill: the background is owned by the keyed 120ms hover
            // transition (motion::hover_bg_transition) — hovering an idle
            // segment eases surface→hover tint and leaving eases back.
            PillState::Idle => {
                let pill = div()
                    .px(tokens::SPACE_10)
                    .h(px(24.0))
                    .rounded(px(self.radius))
                    .border_1()
                    .border_color(self.palette.border)
                    .text_color(self.palette.fg_muted)
                    .text_sm()
                    .cursor_pointer()
                    .child(self.text);
                hover_bg_transition(
                    pill,
                    ("pill-bg", hover_state_key(false, self.hovered)),
                    self.palette.surface,
                    self.palette.hover,
                    self.hovered,
                )
                .into_any_element()
            }
            PillState::Active => div()
                .px(tokens::SPACE_10)
                .h(px(24.0))
                .rounded(px(self.radius))
                .bg(self.palette.accent)
                .text_color(on_accent(self.palette.accent))
                .text_sm()
                .cursor_pointer()
                .child(self.text)
                .into_any_element(),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_pill_tests.rs"]
mod tests;
