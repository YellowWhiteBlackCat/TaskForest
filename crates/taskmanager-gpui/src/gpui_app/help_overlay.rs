//! Keyboard-shortcut help modal (三端对齐: the TUI and iced frontends already
//! ship one; this is the GPUI pane).
//!
//! The rows are derived from the shared shell presentation
//! (`taskmanager_shell::page_help` / `command_help`), so the overlay can never
//! advertise a binding the shared command router does not wire (the same
//! honesty rule the TUI help overlay documents). The GPUI frontend wires the
//! complete shared router, so unlike the TUI every shared command is listed;
//! the frontend-local `F1` / `?` help toggle itself is appended from the
//! shell's local-binding table so the overlay advertises its own opener.
//!
//! State ownership: GPUI derives Help visibility from its single typed
//! window-surface state machine. Toggling is handled in `root/keyboard.rs` as
//! a frontend-local binding (neither `F1` nor
//! `?` exists in the shared `KeyCode` vocabulary — the shell's
//! `shell_local_bindings` treat `?` the same way).

use gpui::ScrollHandle;
use gpui::{
    AnyElement, App, Div, InteractiveElement, IntoElement, ParentElement, Styled, Window, div, px,
};

use taskmanager_ui_contract::{Binding, BindingEntry, FrontendBindingDeclaration, FrontendShape};

use crate::gpui_app::elements;
use crate::gpui_app::theme::mono_font_with_fallback;
use taskmanager_application::i18n;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui::layout::{BoundedScrollRailSpec, bounded_scroll_region_with_rail};
use taskmanager_ui::primitives::section_header::SectionHeader;

/// The modal help panel: a scrim + centered Dialog (via
/// [`elements::dialog_overlay_width`]) carrying the seven-page navigation table
/// and every shared command. `on_close` runs on every close path (X / scrim /
/// ESC / footer), mirroring the Settings modal's contract.
pub fn render_help_overlay(
    t: &Theme,
    window: &mut Window,
    cx: &mut App,
    scroll: ScrollHandle,
    on_close: impl Fn(&mut Window, &mut App) + Clone + 'static,
) -> impl IntoElement {
    let viewport = window.viewport_size();
    let max_dialog_width = (f32::from(viewport.width) - 48.0).max(320.0);
    let dialog_width = max_dialog_width.min(640.0);
    let content_width = (dialog_width - 50.0).max(280.0);
    let content_height = (f32::from(viewport.height) - 150.0).max(260.0);
    let content: AnyElement = bounded_scroll_region_with_rail(
        BoundedScrollRailSpec {
            id: "help-overlay-scroll",
            viewport_selector: "tm-help-overlay-scroll",
            scrollbar_id: "help-overlay-scrollbar",
            scrollbar_selector: "tm-help-overlay-scrollbar",
            track_selector: "tm-help-overlay-scrollbar-track",
            width: Some(px(content_width)),
            max_height: px(content_height),
            scroll,
            palette: t.palette(),
        },
        help_content(t),
    )
    .into_any_element();
    elements::dialog_overlay_width(
        t,
        window,
        cx,
        px(dialog_width),
        i18n::t("settings.keyboard"),
        on_close,
        content,
    )
}

/// The modal body: one "Pages" section (seven shared page-navigation rows) and
/// one "All" section (every shared command plus the frontend-local help
/// toggle). Section titles resolve through existing i18n keys so no new
/// catalog strings are introduced; row copy comes from the shared shell
/// presentation.
fn help_content(t: &Theme) -> Div {
    let pages = taskmanager_shell::page_help();
    let commands = taskmanager_shell::command_help();
    let local_rows = local_binding_rows(t);
    let command_rows = commands
        .into_iter()
        .map(|help| command_row(t, help.label, help.description, help.shortcut))
        .chain(local_rows);
    div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_14,
        ))
        .child(section(
            t,
            i18n::t("settings.keys_pages"),
            div()
                .flex()
                .flex_col()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_6,
                ))
                .children(
                    pages
                        .into_iter()
                        .map(|page| page_row(t, page.label, page.shortcut)),
                ),
        ))
        .child(section(
            t,
            i18n::t("common.all"),
            div()
                .flex()
                .flex_col()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_6,
                ))
                .children(command_rows),
        ))
}

/// The frontend-local bindings this frontend actually wires, appended so the
/// overlay advertises its own `F1` / `?` opener (shell `keys.rs` convention:
/// frontends list their local chords in the shared help data).
fn local_binding_rows(t: &Theme) -> Vec<Div> {
    taskmanager_shell::shell_local_bindings()
        .iter()
        .filter(|binding| binding.shortcut == "?")
        .map(|binding| command_row(t, binding.label, "", "F1 / ?"))
        .collect()
}

/// The GPUI frontend's binding-surface declaration (contract:
/// `taskmanager_ui_contract` keybindings matrix — every contract command
/// gets an explicit bound token or a deliberate `Unbound`; a silent
/// omission is drift). GPUI wires the complete shared router, so every
/// contract command is declared `Binding::Key` with the very token the
/// modal renders: the entries derive from the same `command_help()`
/// presentation `help_content` maps over, so the declaration and the
/// overlay cannot drift apart. The frontend-local `F1` / `?` opener has no
/// `CommandId` and stays a local-binding row (`local_binding_rows`)
/// outside the matrix.
#[must_use]
pub fn binding_declaration() -> FrontendBindingDeclaration {
    FrontendBindingDeclaration {
        frontend: FrontendShape::Gpui,
        entries: taskmanager_shell::command_help()
            .into_iter()
            .map(|help| BindingEntry {
                command: help.command,
                binding: Binding::Key(help.shortcut),
            })
            .collect(),
    }
}

/// One page-navigation row: mono accent shortcut + page label.
fn page_row(t: &Theme, label: &str, shortcut: &str) -> Div {
    row(
        t,
        shortcut,
        label,
        None,
        &format!("tm-help-page:{shortcut}"),
    )
}

/// One command row: mono accent shortcut + label + dim description.
fn command_row(t: &Theme, label: &str, description: &str, shortcut: &str) -> Div {
    row(
        t,
        shortcut,
        label,
        Some(description),
        &format!("tm-help-cmd:{shortcut}"),
    )
}

/// The shared row visual: a bordered card strip with a fixed-width mono
/// shortcut column (accent), the label in the foreground color, and an
/// optional dim description filling the remaining width. Mirrors the Settings
/// shortcut chips' token usage (`card_bg` / `border` / `small_radius`). The
/// debug selector is keyed by section kind + shortcut so headless tests can
/// assert a specific shared row actually rendered (render-geometry
/// breakpoints, same contract as `settings_view` groups).
fn row(t: &Theme, shortcut: &str, label: &str, description: Option<&str>, selector: &str) -> Div {
    let selector = selector.to_string();
    let mut el = div()
        .debug_selector(move || selector.clone())
        .flex()
        .flex_row()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .rounded(taskmanager_ui::theme_binding::absolute(
            tokens::small_radius(t),
        ))
        .border_1()
        .border_color(taskmanager_ui::theme_binding::hsla(t.border))
        .bg(taskmanager_ui::theme_binding::fill(t.card_bg))
        .child(
            div()
                .w(px(88.0))
                .flex_shrink_0()
                .font(mono_font_with_fallback(t))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(t.accent))
                .child(shortcut.to_string()),
        )
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(t.fg))
                .child(label.to_string()),
        );
    if let Some(description) = description
        && !description.is_empty()
    {
        el = el.child(
            div()
                .flex_1()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(t.fg_dim))
                .child(description.to_string()),
        );
    }
    el
}

/// A titled section: bold title above the content block (the same visual
/// contract as the Settings `group` headings).
fn section(t: &Theme, title: &'static str, content: Div) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_8,
        ))
        .child(
            SectionHeader::new(title.to_owned(), t.palette())
                .debug_selector(title)
                .render(),
        )
        .child(content)
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_help_overlay_tests.rs"]
mod tests;
