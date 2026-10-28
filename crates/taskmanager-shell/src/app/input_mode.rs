//! Renderer-neutral ownership of the shell's inline input and information layers.

use super::ShellApp;
use taskmanager_application::AppPage;
use taskmanager_ui_contract::TextEditState;

/// The page-owned query the shared search field edits.
///
/// Applications and Services each keep their own query, so a Services search
/// can never narrow the process list (GPUI reference semantics). The slot is
/// part of the shared state machine: frontends hand keystrokes to the shell
/// without re-implementing which buffer they belong to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SearchSlot {
    #[default]
    Applications,
    Services,
}

/// The sole non-primary keyboard owner carried by the shared shell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ShellInputMode {
    #[default]
    Content,
    Search,
    Help,
    Suggestions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShellInputEvent {
    FocusSearch,
    MoveFocus,
    ToggleHelp,
    ToggleSuggestions,
    CloseSearch,
    DismissInformational,
    Reset,
}

impl ShellInputMode {
    const fn reduce(self, event: ShellInputEvent) -> Self {
        match event {
            ShellInputEvent::FocusSearch => Self::Search,
            ShellInputEvent::MoveFocus => match self {
                Self::Search => Self::Content,
                Self::Content | Self::Help | Self::Suggestions => Self::Search,
            },
            ShellInputEvent::ToggleHelp => match self {
                Self::Help => Self::Content,
                Self::Content | Self::Search | Self::Suggestions => Self::Help,
            },
            ShellInputEvent::ToggleSuggestions => match self {
                Self::Suggestions => Self::Content,
                Self::Content | Self::Search | Self::Help => Self::Suggestions,
            },
            ShellInputEvent::CloseSearch => match self {
                Self::Search => Self::Content,
                Self::Content | Self::Help | Self::Suggestions => self,
            },
            ShellInputEvent::DismissInformational => match self {
                Self::Help | Self::Suggestions => Self::Content,
                Self::Content | Self::Search => self,
            },
            ShellInputEvent::Reset => Self::Content,
        }
    }
}

impl ShellApp {
    #[must_use]
    pub const fn input_mode(&self) -> ShellInputMode {
        self.input_mode
    }

    #[must_use]
    pub const fn search_active(&self) -> bool {
        matches!(self.input_mode, ShellInputMode::Search)
    }

    #[must_use]
    pub const fn help_open(&self) -> bool {
        matches!(self.input_mode, ShellInputMode::Help)
    }

    #[must_use]
    pub const fn suggestions_open(&self) -> bool {
        matches!(self.input_mode, ShellInputMode::Suggestions)
    }

    pub fn open_search(&mut self) {
        self.open_search_for(SearchSlot::Applications);
    }

    /// Open the search field on one page-owned query.
    pub fn open_search_for(&mut self, slot: SearchSlot) {
        self.search_slot = slot;
        self.apply_input_event(ShellInputEvent::FocusSearch);
        let (query, edit) = self.active_search_mut();
        edit.set_caret_at_end(query);
    }

    pub fn toggle_search_focus(&mut self) {
        self.apply_input_event(ShellInputEvent::MoveFocus);
        if self.search_active() {
            // Tab enters the searchable page's own slot; a slot left over from
            // the page the user left must not receive the following keystrokes.
            self.search_slot = if self.application.active_page == AppPage::Services {
                SearchSlot::Services
            } else {
                SearchSlot::Applications
            };
            let (query, edit) = self.active_search_mut();
            edit.set_caret_at_end(query);
        }
    }

    /// The query slot the shared search field currently edits.
    #[must_use]
    pub const fn search_slot(&self) -> SearchSlot {
        self.search_slot
    }

    /// The Services query caret byte, clamped to the query and a UTF-8
    /// boundary.
    #[must_use]
    pub fn services_query_cursor(&self) -> usize {
        self.services_query_edit.caret_in(&self.services_query)
    }

    /// The active slot's query buffer and caret/selection machine.
    pub(super) fn active_search_mut(&mut self) -> (&mut String, &mut TextEditState) {
        match self.search_slot {
            SearchSlot::Applications => (&mut self.query, &mut self.query_edit),
            SearchSlot::Services => (&mut self.services_query, &mut self.services_query_edit),
        }
    }

    pub fn close_search(&mut self) {
        self.apply_input_event(ShellInputEvent::CloseSearch);
    }

    pub fn toggle_help(&mut self) {
        self.apply_input_event(ShellInputEvent::ToggleHelp);
    }

    pub fn toggle_suggestions(&mut self) {
        self.apply_input_event(ShellInputEvent::ToggleSuggestions);
    }

    pub fn dismiss_informational_overlay(&mut self) {
        self.apply_input_event(ShellInputEvent::DismissInformational);
    }

    pub fn reset_input_mode(&mut self) {
        self.apply_input_event(ShellInputEvent::Reset);
    }

    fn apply_input_event(&mut self, event: ShellInputEvent) {
        self.input_mode = self.input_mode.reduce(event);
    }
}

#[cfg(test)]
#[path = "../../tests/headless/shell_app_input_mode.rs"]
mod tests;
