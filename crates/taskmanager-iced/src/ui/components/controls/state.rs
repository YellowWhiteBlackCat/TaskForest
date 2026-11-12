//! Status-to-paint mapping for the shared Iced control family.
//!
//! Iced's own hover/focus/disabled statuses are toolkit signals; the shared
//! `ControlVisualState` is the only vocabulary `control_glass` accepts. This
//! module owns that conversion and no geometry or interaction policy.

use iced::widget::{button, checkbox, text_editor, text_input};
use taskmanager_theme::color::mix;
use taskmanager_theme::{Color, Theme};
use taskmanager_ui_contract::presentation::sizes::ControlRole;

use crate::theme::{self, ControlVisualState};

/// One explicit paint decision, independent of the toolkit's own state
/// tracking. Headless tests can drive every state through the same paint
/// function without a live window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlPaintState {
    Active,
    Hovered,
    Pressed,
    Focused,
    Disabled,
}

impl ControlPaintState {
    #[must_use]
    pub(crate) const fn visual(self) -> ControlVisualState {
        match self {
            Self::Active => ControlVisualState::Active,
            Self::Hovered => ControlVisualState::Hovered,
            Self::Pressed => ControlVisualState::Pressed,
            Self::Focused => ControlVisualState::Focused,
            Self::Disabled => ControlVisualState::Disabled,
        }
    }

    #[must_use]
    pub(crate) const fn from_button(status: button::Status) -> Self {
        Self::from_visual(ControlVisualState::from_ink_priority(
            matches!(status, button::Status::Hovered),
            matches!(status, button::Status::Pressed),
            false,
            matches!(status, button::Status::Disabled),
            false,
        ))
    }

    #[must_use]
    pub(crate) const fn from_text_input(status: text_input::Status) -> Self {
        Self::from_visual(ControlVisualState::from_ink_priority(
            matches!(status, text_input::Status::Hovered),
            false,
            matches!(status, text_input::Status::Focused { .. }),
            matches!(status, text_input::Status::Disabled),
            false,
        ))
    }

    #[must_use]
    pub(crate) const fn from_checkbox(status: checkbox::Status) -> Self {
        Self::from_visual(ControlVisualState::from_ink_priority(
            matches!(status, checkbox::Status::Hovered { .. }),
            false,
            false,
            matches!(status, checkbox::Status::Disabled { .. }),
            false,
        ))
    }

    #[must_use]
    pub(crate) const fn from_text_editor(status: text_editor::Status) -> Self {
        Self::from_visual(ControlVisualState::from_ink_priority(
            matches!(status, text_editor::Status::Hovered),
            false,
            matches!(status, text_editor::Status::Focused { .. }),
            matches!(status, text_editor::Status::Disabled),
            false,
        ))
    }

    #[must_use]
    pub(crate) const fn from_visual(visual: ControlVisualState) -> Self {
        match visual {
            ControlVisualState::Active => Self::Active,
            ControlVisualState::Hovered => Self::Hovered,
            ControlVisualState::Pressed => Self::Pressed,
            ControlVisualState::Focused => Self::Focused,
            ControlVisualState::Disabled => Self::Disabled,
        }
    }
}

/// Variants select the activation's ink family, never a second glass material.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ButtonTone {
    Primary,
    Secondary,
    Danger,
}

/// Convert the shared control glass into Iced's button style. The ink family
/// only reweights the theme's own accent/danger tokens; every border, radius,
/// and shadow belongs to `control_glass`.
#[must_use]
pub(crate) fn button_style(
    theme: &Theme,
    role: ControlRole,
    tone: ButtonTone,
    state: ControlPaintState,
) -> button::Style {
    let glass = theme::control_glass(theme, role, state.visual());
    let palette = theme.palette();
    let accent = match tone {
        ButtonTone::Primary => Some(palette.accent),
        ButtonTone::Danger => Some(palette.danger),
        ButtonTone::Secondary => None,
    };
    let background = match accent {
        Some(fill) if state != ControlPaintState::Disabled => {
            let ink = match state {
                ControlPaintState::Hovered => mix(fill, palette.hover, 0.28),
                ControlPaintState::Pressed => mix(fill, Color::BLACK, 0.22),
                _ => fill,
            };
            iced::Background::Color(crate::theme_binding::color(ink))
        }
        _ => glass.background.unwrap_or_else(|| {
            iced::Background::Color(crate::theme_binding::color(palette.surface))
        }),
    };
    let text_color = match accent {
        Some(_) if state != ControlPaintState::Disabled => {
            crate::theme_binding::color(theme.accent_text)
        }
        _ => glass
            .text_color
            .unwrap_or_else(|| crate::theme_binding::color(palette.fg)),
    };
    let mut border = glass.border;
    // The control family's corner radius is the CONTROL radius tier (the
    // same one the GPUI button/input primitives paint), not the panel tier
    // the shared card glass carries.
    border.radius = theme::scaled_radius(theme, palette.control_radius).into();
    button::Style {
        background: Some(background),
        text_color,
        border,
        shadow: glass.shadow,
        snap: true,
    }
}
