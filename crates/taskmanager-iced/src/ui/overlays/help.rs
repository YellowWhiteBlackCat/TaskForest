//! Keyboard-shortcut and help overlay modal for Iced (GPUI/TUI parity).
//!
//! Rows are derived from the shared shell presentation
//! (`taskmanager_shell::page_help` / `command_help`), ensuring the overlay
//! advertises exactly the keybindings wired by the shared router and the
//! frontend-local `F1` / `?` opener.

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_shell::{CommandHelp, PageHelp, command_help, page_help};
use taskmanager_theme::{Theme, tokens};
use taskmanager_ui_contract::{Binding, BindingEntry, FrontendBindingDeclaration, FrontendShape};

use crate::app::Message;
use crate::theme;

/// The Iced frontend's binding-surface declaration (contract:
/// `taskmanager_ui_contract` keybindings matrix — every contract command
/// gets an explicit bound token or a deliberate `Unbound`; a silent
/// omission is drift). Iced wires the complete shared router, so every
/// contract command is declared `Binding::Key` with the very token the
/// overlay renders: the entries derive from the same `command_help()`
/// presentation the overlay joins for labels, so the declaration and the
/// rendered rows cannot drift apart. The frontend-local `F1` / `?` opener
/// has no `CommandId` and stays the local row below, outside the matrix.
pub(crate) fn binding_declaration() -> FrontendBindingDeclaration {
    FrontendBindingDeclaration {
        frontend: FrontendShape::Iced,
        entries: command_help()
            .into_iter()
            .map(|help| BindingEntry {
                command: help.command,
                binding: Binding::Key(help.shortcut),
            })
            .collect(),
    }
}

/// Render the comprehensive keyboard shortcuts help overlay.
pub(crate) fn help_overlay<'a>(
    theme_snapshot: &'a Theme,
    appear: f32,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let pages = page_help();
    let commands = command_help();
    let declaration = binding_declaration();

    let page_elements: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = pages
        .into_iter()
        .map(|p| page_help_row(theme_snapshot, p))
        .collect();

    // The declaration is the single list: a command row renders exactly
    // when this frontend declares it bound, joined back to the shared
    // presentation for label and description. The coverage gate in the
    // test module asserts the declaration binds every contract command,
    // so this fold never silently drops a row.
    let command_elements: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = declaration
        .entries
        .iter()
        .filter_map(|entry| {
            let shortcut = entry.binding.key_token()?;
            let help = commands.iter().find(|help| help.command == entry.command)?;
            Some(command_help_row(theme_snapshot, *help, shortcut))
        })
        .chain(std::iter::once(local_help_row(theme_snapshot)))
        .collect();

    let body = column![
        help_section_heading(theme_snapshot, t("settings.keys_pages")),
        column(page_elements).spacing(2),
        help_section_heading(theme_snapshot, t("common.all")),
        column(command_elements).spacing(2),
    ]
    .spacing(10)
    .width(Length::Fill);

    super::modal_overlay(
        theme_snapshot,
        t("settings.keyboard"),
        t("settings.keyboard_subtitle"),
        scrollable(body)
            .height(Length::Fixed(380.0))
            .width(Length::Fill)
            .into(),
        appear,
    )
}

fn help_section_heading<'a>(
    theme_snapshot: &'a Theme,
    title: &'static str,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    text(title)
        .size(f32::from(tokens::FONT_13))
        .color(theme::color(theme_snapshot.palette().accent))
        .into()
}

fn key_badge<'a>(
    theme_snapshot: &'a Theme,
    shortcut: &'a str,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    container(text(shortcut).size(f32::from(tokens::FONT_11)))
        .style(move |_| theme::panel_style(theme_snapshot))
        .padding([2, 6])
        .into()
}

fn page_help_row<'a>(
    theme_snapshot: &'a Theme,
    page: PageHelp,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    row![
        row![key_badge(theme_snapshot, page.shortcut)].width(Length::Fixed(90.0)),
        text(page.label)
            .size(f32::from(tokens::FONT_12))
            .width(Length::Fill),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .padding(2)
    .width(Length::Fill)
    .into()
}

fn command_help_row<'a>(
    theme_snapshot: &'a Theme,
    help: CommandHelp,
    shortcut: &'a str,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    row![
        row![key_badge(theme_snapshot, shortcut)].width(Length::Fixed(90.0)),
        text(help.label)
            .size(f32::from(tokens::FONT_12))
            .width(Length::Fixed(160.0)),
        text(help.description)
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fill),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .padding(2)
    .width(Length::Fill)
    .into()
}

fn local_help_row<'a>(
    theme_snapshot: &'a Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    row![
        row![key_badge(theme_snapshot, "F1 / ?")].width(Length::Fixed(90.0)),
        text(t("settings.keyboard"))
            .size(f32::from(tokens::FONT_12))
            .width(Length::Fixed(160.0)),
        text(t("settings.keyboard_subtitle"))
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fill),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .padding(2)
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/overlays/help_tests.rs"]
mod tests;
