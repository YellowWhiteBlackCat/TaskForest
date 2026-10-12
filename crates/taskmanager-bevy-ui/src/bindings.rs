//! Bevy's explicit command-binding declaration.
//!
//! The Bevy input adapter routes shared keys through the shell command table.
//! This declaration records each shared command binding from the shell's
//! command table so the four-frontend coverage matrix never mistakes an
//! absent route for a forgotten entry.

use taskmanager_application::CommandId;
use taskmanager_shell::command_help;
use taskmanager_ui_contract::{Binding, BindingEntry, FrontendBindingDeclaration, FrontendShape};

/// Declare every shared command the Bevy input seam knows about.
#[must_use]
pub fn binding_declaration() -> FrontendBindingDeclaration {
    let help = command_help();
    let entries = CommandId::ALL
        .into_iter()
        .map(|command| {
            let binding = help
                .iter()
                .find(|item| item.command == command)
                .map_or(Binding::Unbound, |item| Binding::Key(item.shortcut));
            BindingEntry { command, binding }
        })
        .collect();
    FrontendBindingDeclaration {
        frontend: FrontendShape::Bevy,
        entries,
    }
}

#[cfg(test)]
#[path = "../tests/headless/bindings.rs"]
mod tests;
