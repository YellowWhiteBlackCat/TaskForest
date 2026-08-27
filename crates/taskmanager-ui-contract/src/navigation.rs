//! Canonical page navigation metadata for renderer adapters.
//!
//! The application owns page routing and command behavior. This module owns
//! the stable presentation relationship between those routes and semantic
//! icons/message keys; each renderer decides how to draw or localize it.

use taskmanager_application::{AppPage, CommandBinding, CommandId, KeyChord, default_bindings};

use crate::{IconId, MessageKey, descriptor};

/// One shared application-shell page entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageDescriptor {
    pub page: AppPage,
    pub command: CommandId,
    pub icon: IconId,
    pub label: MessageKey,
    pub description: MessageKey,
}

impl PageDescriptor {
    /// Build the complete semantic descriptor for one application page.
    #[must_use]
    pub const fn for_page(page: AppPage) -> Self {
        let (command, icon) = match page {
            AppPage::Performance => (CommandId::ShowPerformance, IconId::Performance),
            AppPage::Applications => (CommandId::ShowApplications, IconId::Applications),
            AppPage::Services => (CommandId::ShowServices, IconId::Services),
            AppPage::System => (CommandId::ShowSystem, IconId::System),
            AppPage::Startup => (CommandId::ShowStartup, IconId::Startup),
            AppPage::Users => (CommandId::ShowUsers, IconId::Users),
            AppPage::AppHistory => (CommandId::ShowAppHistory, IconId::History),
        };
        let presentation = descriptor(command);
        Self {
            page,
            command,
            icon,
            label: presentation.label,
            description: presentation.description,
        }
    }
}

/// Resolve one page without making renderers duplicate the route table.
#[must_use]
pub const fn page_descriptor(page: AppPage) -> PageDescriptor {
    PageDescriptor::for_page(page)
}

/// All shared application pages in the application-owned canonical order.
#[must_use]
pub fn page_descriptors() -> [PageDescriptor; 7] {
    AppPage::ALL.map(PageDescriptor::for_page)
}

/// Find the default shortcut belonging to a shared page command.
#[must_use]
pub fn page_shortcut(page: AppPage) -> Option<CommandBinding> {
    let command = page_descriptor(page).command;
    default_bindings()
        .iter()
        .copied()
        .find(|binding| binding.command == command)
}

/// Extract only the typed chord when a frontend does not need the scope.
#[must_use]
pub fn page_key_chord(page: AppPage) -> Option<KeyChord> {
    page_shortcut(page).map(|binding| binding.chord)
}

#[cfg(test)]
#[path = "../tests/headless/ui_navigation.rs"]
mod tests;
