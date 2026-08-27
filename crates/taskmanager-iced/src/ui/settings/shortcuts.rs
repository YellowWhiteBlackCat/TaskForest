//! The Settings page's read-only keyboard-shortcut legend.
//!
//! The rows derive from the SAME Iced binding declaration the F1 help
//! overlay renders (`taskmanager_ui_contract` keybindings matrix — never a
//! copied key table): every contract command shows its declared key token
//! or an explicit not-bound state, joined back to the shared
//! `command_help()` presentation for the label.

use iced::Length;
use iced::widget::{column, container, row, text};
use taskmanager_shell::{CommandHelp, command_help};
use taskmanager_ui_contract::FrontendBindingDeclaration;

use super::*;
use crate::ui::overlays::binding_declaration;

/// One rendered shortcut row: the shared command label plus the declared
/// key token, or `None` when the declaration marks the command deliberately
/// unbound in this frontend.
pub(super) struct ShortcutRow {
    pub(super) label: &'static str,
    pub(super) keys: Option<&'static str>,
}

/// Build the row models from one binding declaration. A declared command
/// the shared presentation cannot label is dropped (the same join the help
/// overlay performs); the declaration is built from `command_help()` itself,
/// so in practice every entry joins.
pub(super) fn shortcut_rows(declaration: &FrontendBindingDeclaration) -> Vec<ShortcutRow> {
    let commands = command_help();
    declaration
        .entries
        .iter()
        .filter_map(|entry| {
            let help: &CommandHelp = commands.iter().find(|help| help.command == entry.command)?;
            Some(ShortcutRow {
                label: help.label,
                keys: entry.binding.key_token(),
            })
        })
        .collect()
}

/// The read-only legend inside the General group. No control, no focus
/// stop: this surface documents the wired bindings, it does not edit them.
pub(super) fn shortcut_section<'a>(theme_snapshot: &'a Theme) -> IcedElement<'a> {
    let rows = shortcut_rows(&binding_declaration());
    let elements: Vec<IcedElement<'a>> = rows
        .iter()
        .map(|row| shortcut_row(theme_snapshot, row))
        .collect();
    column(elements).spacing(f32::from(tokens::SPACE_2)).into()
}

fn shortcut_row<'a>(theme_snapshot: &'a Theme, row: &ShortcutRow) -> IcedElement<'a> {
    let badge: IcedElement<'a> = match row.keys {
        Some(token) => container(text(token).size(f32::from(tokens::FONT_11)))
            .style(move |_| crate::theme::panel_style(theme_snapshot))
            .padding([2, 6])
            .into(),
        None => text(taskmanager_application::i18n::t("common.none"))
            .size(f32::from(tokens::FONT_11))
            .color(crate::theme::muted_text_color(theme_snapshot))
            .into(),
    };
    row![
        container(badge).width(Length::Fixed(90.0)),
        text(row.label).size(f32::from(tokens::FONT_12)),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}
