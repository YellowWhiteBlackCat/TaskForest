//! Status-to-paint mapping for the shared Iced control family.
//!
//! Iced's own hover/focus/disabled statuses are toolkit signals; the shared
//! `ControlInkPriority` fold is the ONE precedence owner that turns them
//! into an ink, and `ControlVisualState` is the only vocabulary `control_glass`
//! accepts. This module only PROJECTS each toolkit's status into that shared
//! priority — it never resolves a verdict of its own — and owns no geometry
//! or interaction policy.

use iced::widget::{button, checkbox, pick_list, slider, text_editor, text_input};
use taskmanager_theme::color::mix;
use taskmanager_theme::{Color, Theme};
use taskmanager_ui_contract::presentation::control_ink::ControlInkPriority;
use taskmanager_ui_contract::presentation::sizes::ControlRole;

use crate::theme::{self, ControlVisualState};

/// The shared ink a native button status resolves to.
#[must_use]
pub(crate) const fn button_ink(status: button::Status) -> ControlVisualState {
    ControlVisualState::from_priority(ControlInkPriority {
        hovered: matches!(status, button::Status::Hovered),
        pressed: matches!(status, button::Status::Pressed),
        focused: false,
        disabled: matches!(status, button::Status::Disabled),
        selected: false,
    })
}

/// The shared ink a native single-line input status resolves to. A focused
/// field outranks the pointer resting on it, so the keyboard modality ring
/// stays readable while the pointer happens to be inside the field.
#[must_use]
pub(crate) const fn text_input_ink(status: text_input::Status) -> ControlVisualState {
    ControlVisualState::from_priority(ControlInkPriority {
        hovered: matches!(status, text_input::Status::Hovered),
        pressed: false,
        focused: matches!(status, text_input::Status::Focused { .. }),
        disabled: matches!(status, text_input::Status::Disabled),
        selected: false,
    })
}

/// The shared ink a native multi-line editor status resolves to.
#[must_use]
pub(crate) const fn text_editor_ink(status: text_editor::Status) -> ControlVisualState {
    ControlVisualState::from_priority(ControlInkPriority {
        hovered: matches!(status, text_editor::Status::Hovered),
        pressed: false,
        focused: matches!(status, text_editor::Status::Focused { .. }),
        disabled: matches!(status, text_editor::Status::Disabled),
        selected: false,
    })
}

/// The shared ink a native checkbox status resolves to.
#[must_use]
pub(crate) const fn checkbox_ink(status: checkbox::Status) -> ControlVisualState {
    ControlVisualState::from_priority(ControlInkPriority {
        hovered: matches!(status, checkbox::Status::Hovered { .. }),
        pressed: false,
        focused: false,
        disabled: matches!(status, checkbox::Status::Disabled { .. }),
        selected: false,
    })
}

/// The shared ink a native slider status resolves to. A dragged slider is the
/// pressed ink: the thumb is under the pointer by definition.
#[must_use]
pub(crate) const fn slider_ink(status: slider::Status) -> ControlVisualState {
    ControlVisualState::from_priority(ControlInkPriority {
        hovered: matches!(status, slider::Status::Hovered),
        pressed: matches!(status, slider::Status::Dragged),
        focused: false,
        disabled: false,
        selected: false,
    })
}

/// The shared ink a native select trigger resolves to. An open dropdown keeps
/// the focused ink while the pointer rests on the trigger.
#[must_use]
pub(crate) const fn pick_list_ink(status: pick_list::Status) -> ControlVisualState {
    ControlVisualState::from_priority(ControlInkPriority {
        hovered: matches!(status, pick_list::Status::Hovered),
        pressed: false,
        focused: matches!(status, pick_list::Status::Opened { .. }),
        disabled: false,
        selected: false,
    })
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
    state: ControlVisualState,
) -> button::Style {
    let glass = theme::control_glass(theme, role, state);
    let palette = theme.palette();
    let accent = match tone {
        ButtonTone::Primary => Some(palette.accent),
        ButtonTone::Danger => Some(palette.danger),
        ButtonTone::Secondary => None,
    };
    let background = match accent {
        Some(fill) if state != ControlVisualState::Disabled => {
            let ink = match state {
                ControlVisualState::Hovered => mix(fill, palette.hover, 0.28),
                ControlVisualState::Pressed => mix(fill, Color::BLACK, 0.22),
                _ => fill,
            };
            iced::Background::Color(crate::theme_binding::color(ink))
        }
        _ => glass.background.unwrap_or_else(|| {
            iced::Background::Color(crate::theme_binding::color(palette.surface))
        }),
    };
    let text_color = match accent {
        Some(_) if state != ControlVisualState::Disabled => {
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
