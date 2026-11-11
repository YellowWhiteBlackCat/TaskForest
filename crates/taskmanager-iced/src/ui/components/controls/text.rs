//! The single-line text input over Iced's native widget.
//!
//! The native widget keeps caret, IME and selection behavior; the only paint
//! seam reads `control_glass` through the shared `Input` role. Geometry comes
//! from `ControlRole` resolution, never a second size ladder.

use iced::widget::{text_editor, text_input};
use iced::{Background, Length};
use taskmanager_theme::{Theme, tokens};
use taskmanager_ui_contract::presentation::sizes::ControlRole;

use super::state::ControlPaintState;
use crate::app::{FocusTarget, Message};
use crate::focus::focus_id;
use crate::theme::{self, control_glass};
use crate::ui::components::IcedElement;

/// A single-line input. `id` binds the field to the shared focus shell's
/// stable operation id; disabled removes both input and submit callbacks
/// without changing the measured extent.
#[must_use]
pub(crate) fn plain_text_input<'a>(
    theme_snapshot: &'a Theme,
    target: Option<FocusTarget>,
    placeholder: &'a str,
    value: &'a str,
    on_change: impl Fn(String) -> Message + 'a,
    on_submit: Option<Message>,
    disabled: bool,
) -> IcedElement<'a> {
    let extent = theme::control_extent(theme_snapshot, ControlRole::Input);
    let padding_x = theme::control_padding_x(theme_snapshot, ControlRole::Input);
    let font_size = f32::from(tokens::FONT_13);
    let mut input = text_input::TextInput::new(placeholder, value)
        .padding([(extent - font_size * 1.3).max(0.0) * 0.5, padding_x])
        .size(font_size)
        .style(move |_, status| {
            text_input_style(
                theme_snapshot,
                ControlRole::Input,
                ControlPaintState::from_text_input(status),
            )
        })
        .width(Length::Fill);
    if let Some(target) = target {
        input = input.id(iced::widget::Id::from(focus_id(target)));
    }
    if !disabled {
        input = input.on_input(on_change);
    }
    if let Some(message) = on_submit.filter(|_| !disabled) {
        input = input.on_submit(message);
    }
    input.into()
}

#[must_use]
pub(crate) fn text_input_style(
    theme: &Theme,
    role: ControlRole,
    state: ControlPaintState,
) -> text_input::Style {
    let glass = control_glass(theme, role, state.visual());
    let palette = theme.palette();
    let ink = glass
        .text_color
        .unwrap_or_else(|| crate::theme_binding::color(palette.fg));
    text_input::Style {
        background: glass
            .background
            .unwrap_or_else(|| Background::Color(crate::theme_binding::color(palette.surface))),
        border: glass.border,
        icon: ink,
        placeholder: ink,
        value: ink,
        selection: crate::theme_binding::color(palette.selection),
    }
}

/// The multi-line editor over the same `ControlRole::Input` glass, padding,
/// type ladder and focus ring the single-line control spends. Every edit
/// becomes one typed application message; the renderer owns no second visual
/// state, dictionary or geometry ladder.
#[must_use]
pub(crate) fn multi_line_text_input<'a>(
    theme_snapshot: &'a Theme,
    target: Option<FocusTarget>,
    content: &'a text_editor::Content,
    placeholder: &'a str,
    on_action: impl Fn(text_editor::Action) -> Message + 'a,
    on_submit: Option<Message>,
    disabled: bool,
) -> IcedElement<'a> {
    let extent = theme::control_extent(theme_snapshot, ControlRole::Input);
    let padding_x = theme::control_padding_x(theme_snapshot, ControlRole::Input);
    let font_size = f32::from(tokens::FONT_13);
    let mut editor = text_editor::TextEditor::new(content)
        .placeholder(placeholder)
        .padding([(extent - font_size * 1.3).max(0.0) * 0.5, padding_x])
        .size(font_size)
        .font(crate::theme_binding::ui_font(theme_snapshot))
        .height(Length::Fixed(extent * 3.0))
        .style(move |_, status| {
            text_editor_style(
                theme_snapshot,
                ControlRole::Input,
                ControlPaintState::from_text_editor(status),
            )
        });
    if let Some(target) = target {
        editor = editor.id(iced::widget::Id::from(focus_id(target)));
    }
    if !disabled {
        editor = editor.on_action(on_action);
    }
    if let Some(message) = on_submit.filter(|_| !disabled) {
        editor = editor.key_binding(move |key_press| {
            let submit = message.clone();
            if key_press.modifiers.command()
                && matches!(
                    key_press.modified_key.as_ref(),
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter)
                )
            {
                Some(text_editor::Binding::Custom(submit))
            } else {
                text_editor::Binding::from_key_press(key_press)
            }
        });
    }
    editor.into()
}

#[must_use]
pub(crate) fn text_editor_style(
    theme: &Theme,
    role: ControlRole,
    state: ControlPaintState,
) -> text_editor::Style {
    let glass = control_glass(theme, role, state.visual());
    let palette = theme.palette();
    let ink = glass
        .text_color
        .unwrap_or_else(|| crate::theme_binding::color(palette.fg));
    text_editor::Style {
        background: glass
            .background
            .unwrap_or_else(|| Background::Color(crate::theme_binding::color(palette.surface))),
        border: glass.border,
        placeholder: ink,
        value: ink,
        selection: crate::theme_binding::color(palette.selection),
    }
}
