//! Terminal frontend's explicit command-binding declaration.
//!
//! Contract: [`taskmanager_ui_contract`] keybindings matrix — every
//! contract-known command gets an explicit [`Binding::Key`] token or a
//! deliberate [`Binding::Unbound`]; a silent omission is drift. This module
//! derives the terminal shape's declaration from the same single source the
//! help overlay and the command palette render (the shared router table via
//! [`crate::command_help`]); it does not duplicate a second list. The
//! terminal-local chords ([`crate::shell_local_bindings`],
//! [`crate::ui::help::TUI_LOCAL_BINDINGS`]) have no `CommandId` by design
//! and stay outside the matrix, covered instead by the help-row parity
//! test at the bottom of this file.

use taskmanager_application::CommandId;
use taskmanager_ui_contract::{Binding, BindingEntry, FrontendBindingDeclaration, FrontendShape};

/// Shared-router commands the terminal shape deliberately does not wire.
/// The honesty rationale lives with the help overlay
/// ([`crate::ui::help::help_rows`]): the TUI answers its end-task
/// confirmation locally with `y` / `n` / `Esc` *before* the router is
/// consulted, and a terminal has no sidebar surface — so advertising
/// `Enter`-as-confirm or `F9` would promise chords this frontend leaves
/// dead.
const DELIBERATELY_UNBOUND: [CommandId; 2] = [CommandId::Confirm, CommandId::ToggleSidebar];

/// The terminal shape's binding-surface declaration: one explicit entry per
/// contract command. Bound entries mirror [`crate::command_help`] (the
/// conflict-checked shared router presentation); the two terminal
/// exemptions are declared [`Binding::Unbound`] rather than silently
/// dropped, so the coverage gate can tell a choice from an omission.
#[must_use]
pub fn binding_declaration() -> FrontendBindingDeclaration {
    let shared = crate::command_help();
    let entries = CommandId::ALL
        .into_iter()
        .map(|command| {
            let binding = if DELIBERATELY_UNBOUND.contains(&command) {
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
