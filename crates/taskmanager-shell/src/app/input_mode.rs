//! Renderer-neutral ownership of the shell's inline input and information layers.

use super::ShellApp;

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
        self.apply_input_event(ShellInputEvent::FocusSearch);
    }

    pub fn toggle_search_focus(&mut self) {
        self.apply_input_event(ShellInputEvent::MoveFocus);
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
