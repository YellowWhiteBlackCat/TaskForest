//! Terminal frontend's explicit command-binding declaration.
//!
//! Contract: [`taskmanager_ui_contract`] keybindings matrix — every
//! contract-known command gets an explicit [`Binding::Key`] token or a
//! deliberate [`Binding::Unbound`]; a silent omission is drift. This module
//! derives the terminal shape's declaration from the same single source the
//! help overlay and the command palette render (the shared router table via
//! [`taskmanager_shell::presentation::command_help`]); it does not duplicate a second list. The
//! terminal-local chords have no `CommandId` by design and stay outside the
//! matrix: the shell's five characters live in
//! [`taskmanager_shell::shell_local_bindings`], and every TUI-local command chord —
//! including its direct keyboard dispatch — lives in
//! [`crate::command_palette::TUI_LOCAL_COMMANDS`], covered by the help/palette
//! parity tests and the per-item binding-matrix execution tests.

use taskmanager_application::CommandId;
use taskmanager_application::i18n::t;
use taskmanager_ui_contract::{Binding, BindingEntry, FrontendBindingDeclaration, FrontendShape};

/// Shared-router commands the terminal shape deliberately does not wire.
/// Confirmation uses `y` / `n` / `Esc`, and the terminal has a resource
/// selector instead of a desktop sidebar. Keeping both explicit prevents
/// help/coverage from claiming a dead shortcut.
pub(crate) const DELIBERATELY_UNBOUND: [CommandId; 2] =
    [CommandId::Confirm, CommandId::ToggleSidebar];

/// Whether a shared command is intentionally absent from the TUI binding
/// surface. Help and binding declaration both call this named authority.
#[must_use]
pub(crate) fn is_deliberately_unbound(command: CommandId) -> bool {
    DELIBERATELY_UNBOUND.contains(&command)
}

// ── Action-menu footer hint vocabulary (TUI-003) ────────────────────────────
//
// The six action-menu overlays (service / process / batch / session /
// startup / column) take their bottom footer chord/label pairs from this
// one declaration table, exactly as [`binding_declaration`] is the one
// declaration table for the command matrix: a footer copy change is a
// one-place edit, and two menus can never drift apart.

/// One action-menu footer hint: the painted chord token plus the i18n
/// catalog key of its label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MenuHint {
    /// The chord token exactly as painted (padding is part of the glyph).
    pub chord: &'static str,
    /// The shared i18n catalog key of the label text.
    pub label_key: &'static str,
    /// Whether this hint continues the line: its label is followed by the
    /// `·` separator. The last hint of every footer renders bare.
    pub joins: bool,
}

impl MenuHint {
    /// The rendered `(chord, label)` pair shaped for the shared `KeyHint`
    /// component: the label carries its surrounding spaces and, when
    /// [`MenuHint::joins`], the trailing separator.
    #[must_use]
    pub(crate) fn pair(self) -> (&'static str, String) {
        let label = t(self.label_key);
        let text = if self.joins {
            format!(" {label} · ")
        } else {
            format!(" {label}")
        };
        (self.chord, text)
    }
}

/// The generic action-menu footer (↑↓ move · Enter select · Esc cancel),
/// shared verbatim by the service, process, batch, and session menus.
pub(crate) const ACTION_MENU_HINTS: [MenuHint; 3] = [
    MenuHint {
        chord: " ↑↓ ",
        label_key: "menu.word_move",
        joins: true,
    },
    MenuHint {
        chord: "Enter",
        label_key: "menu.word_select",
        joins: true,
    },
    MenuHint {
        chord: "Esc",
        label_key: "menu.word_cancel",
        joins: false,
    },
];

/// The startup menu's footer: the same three catalog labels over the
/// space-padded chord tokens the menu's footer pins.
pub(crate) const STARTUP_MENU_HINTS: [MenuHint; 3] = [
    MenuHint {
        chord: " ↑↓ ",
        label_key: "menu.word_move",
        joins: true,
    },
    MenuHint {
        chord: " Enter ",
        label_key: "menu.word_select",
        joins: true,
    },
    MenuHint {
        chord: " Esc ",
        label_key: "menu.word_cancel",
        joins: false,
    },
];

/// The column-visibility menu's footer: its single combined
/// toggle-then-Esc-closes hint.
pub(crate) const COLUMN_MENU_HINTS: [MenuHint; 1] = [MenuHint {
    chord: " Enter ",
    label_key: "tui.toggle_esc_close",
    joins: false,
}];

/// The painted chord/label pairs of a hint table slice, in order — the one
/// adapter from this vocabulary into the `KeyHint` component.
#[must_use]
pub(crate) fn menu_hint_pairs(hints: &[MenuHint]) -> Vec<(&'static str, String)> {
    hints.iter().copied().map(MenuHint::pair).collect()
}

/// The terminal shape's binding-surface declaration: one explicit entry per
/// contract command. Bound entries mirror [`taskmanager_shell::presentation::command_help`] (the
/// conflict-checked shared router presentation); the explicitly unbound
/// exemptions are declared [`Binding::Unbound`] rather than silently
/// dropped, so the coverage gate can tell a choice from an omission.
#[must_use]
pub fn binding_declaration() -> FrontendBindingDeclaration {
    let shared = taskmanager_shell::presentation::command_help();
    let entries = CommandId::ALL
        .into_iter()
        .map(|command| {
            let binding = if is_deliberately_unbound(command) {
                Binding::Unbound
            } else {
                shared
                    .iter()
                    .find(|help| help.command == command)
                    .map_or(Binding::Unbound, |help| Binding::Key(help.shortcut))
            };
            BindingEntry { command, binding }
        })
        .collect();
    FrontendBindingDeclaration {
        frontend: FrontendShape::Tui,
        entries,
    }
}

#[cfg(test)]
#[path = "../tests/gui/bindings_tests.rs"]
mod tests;
