//! Shared command presentation metadata.

use crate::{IconId, MessageKey};
use taskmanager_application::CommandId;

/// Presentation metadata for one command.
///
/// The application owns command identity and behavior; this contract owns
/// only semantic presentation so every frontend can resolve the same text
/// and icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandDescriptor {
    pub label: MessageKey,
    pub description: MessageKey,
    pub icon: Option<IconId>,
}

impl CommandDescriptor {
    /// Default presentation for a semantic command: the label/description
    /// message keys derive from the command identity itself; the icon is
    /// this contract's one per-command lookup.
    #[must_use]
    pub const fn for_command(command: CommandId) -> Self {
        let icon = match command {
            CommandId::FocusSearch => Some(IconId::Search),
            CommandId::PageUp => Some(IconId::NavigateUp),
            CommandId::PageDown => Some(IconId::NavigateDown),
            CommandId::ArrowUp => Some(IconId::NavigateUp),
            CommandId::ArrowDown => Some(IconId::NavigateDown),
            CommandId::FocusNext | CommandId::FocusPrevious => Some(IconId::Focus),
            CommandId::ShowPerformance => Some(IconId::Performance),
            CommandId::ShowApplications => Some(IconId::Applications),
            CommandId::ShowServices => Some(IconId::Services),
            CommandId::ShowSystem => Some(IconId::System),
            CommandId::ShowStartup => Some(IconId::Startup),
            CommandId::ShowUsers => Some(IconId::Users),
            CommandId::ShowAppHistory => Some(IconId::History),
            CommandId::ShowAlerts => Some(IconId::Alert),
            CommandId::Refresh => Some(IconId::Refresh),
            CommandId::EndTask => Some(IconId::EndTask),
            CommandId::OpenProperties => Some(IconId::Properties),
            CommandId::ShowSystemAbout => Some(IconId::System),
            CommandId::Dismiss => Some(IconId::Close),
            CommandId::Confirm => Some(IconId::CircleCheck),
            CommandId::TogglePause => Some(IconId::Pause),
            CommandId::ToggleSidebar => Some(IconId::Sidebar),
            CommandId::MoveToFirst => Some(IconId::NavigateUp),
            CommandId::MoveToLast => Some(IconId::NavigateDown),
            // Clipboard export of the selected row: the closest semantic
            // icon in the contract vocabulary (no dedicated Copy variant).
            CommandId::CopySelectedRow => Some(IconId::Export),
        };
        Self::new(
            MessageKey::CommandLabel(command),
            MessageKey::CommandDescription(command),
            icon,
        )
    }

    #[must_use]
    pub const fn new(label: MessageKey, description: MessageKey, icon: Option<IconId>) -> Self {
        Self {
            label,
            description,
            icon,
        }
    }
}

/// Default semantic presentation for an application command.
#[must_use]
pub const fn descriptor(command: CommandId) -> CommandDescriptor {
    CommandDescriptor::for_command(command)
}

#[cfg(test)]
#[path = "../tests/headless/ui_command.rs"]
mod tests;
