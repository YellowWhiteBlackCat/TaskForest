//! Bevy's explicit command-binding declaration.
//!
//! The Bevy input adapter routes shared keys through the shell command table.
//! This declaration records the few commands that have no Bevy surface yet so
//! the four-frontend coverage matrix never mistakes an absent route for a
//! forgotten entry.

use taskmanager_application::CommandId;
use taskmanager_shell::command_help;
use taskmanager_ui_contract::{Binding, BindingEntry, FrontendBindingDeclaration, FrontendShape};

/// Shared commands deliberately not offered by the current Bevy surface.
///
/// Bevy has no sidebar toggle, system-about surface, or clipboard adapter in
/// this milestone. The entries remain explicit rather than being dropped from
/// the declaration.
const DELIBERATELY_UNBOUND: [CommandId; 3] = [
    CommandId::ShowSystemAbout,
    CommandId::ToggleSidebar,
    CommandId::CopySelectedRow,
];

/// Whether a shared command is intentionally absent from the Bevy shape.
#[must_use]
pub fn is_deliberately_unbound(command: CommandId) -> bool {
    DELIBERATELY_UNBOUND.contains(&command)
}

/// Declare every shared command the Bevy input seam knows about.
#[must_use]
pub fn binding_declaration() -> FrontendBindingDeclaration {
    let help = command_help();
    let entries = CommandId::ALL
        .into_iter()
        .map(|command| {
            let binding = if is_deliberately_unbound(command) {
                Binding::Unbound
            } else {
                help.iter()
                    .find(|item| item.command == command)
                    .map_or(Binding::Unbound, |item| Binding::Key(item.shortcut))
            };
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
