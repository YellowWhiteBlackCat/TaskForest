//! ShellApp keyboard entry points: router dispatch and the bare-key
//! table-page navigation layer. Split from `app.rs` (line-budget).

use super::*;

impl ShellApp {
    #[must_use]
    pub fn dispatch_key(&mut self, event: ShellKeyEvent) -> InputDispatch {
        let context = CommandContext {
            scope: if self.page() == AppPage::Applications {
                CommandScope::ProcessList
            } else {
                CommandScope::Shell
            },
            text_input_focused: self.search_active(),
            overlay_present: self.application.interaction.is_open(),
            process_selected: self.selected_process_identity().is_some(),
        };
        let Some(action) = route_key(event, context) else {
            return InputDispatch::Unhandled;
        };
        InputDispatch::consumed(self.apply_action(action))
    }

    /// Handle the frontend-local key/character bindings shared by every
    /// frontend (ADR-027): modal overlay keys, quit, help, sort, suggestions,
    /// search-field editing and selection movement. The explicit result keeps
    /// "not a shell binding" separate from "consumed without platform work".
    ///
    /// Modal precedence (matches the TUI's historical behaviour):
    /// pending-end > help overlay > suggestions overlay > search field >
    /// plain bindings.
    pub fn handle_local_char(&mut self, character: char, modifiers: Modifiers) -> InputDispatch {
        if let confirmation_gates::GateRouting::Consumed(effect) =
            confirmation_gates::route_armed_gate(self, character)
        {
            return InputDispatch::consumed(effect);
        }
        if self.help_open() {
            if character == '?' {
                self.toggle_help();
            }
            return InputDispatch::Consumed;
        }
        if self.suggestions_open() {
            if character == 'T' {
                self.toggle_suggestions();
            }
            return InputDispatch::Consumed;
        }
        if self.search_active() {
            if !character.is_control() && modifiers == Modifiers::NONE {
                self.push_search_char(character);
            }
            return InputDispatch::Consumed;
        }
        match character {
            'q' if modifiers == Modifiers::NONE => {
                self.request_quit(QuitReason::Keyboard);
                InputDispatch::Consumed
            }
            '?' => {
                self.toggle_help();
                InputDispatch::Consumed
            }
            's' if modifiers == Modifiers::NONE => {
                self.cycle_sort_column();
                InputDispatch::Consumed
            }
            'S' if !modifiers.control && !modifiers.alt => {
                self.toggle_sort_direction();
                InputDispatch::Consumed
            }
            'T' if !modifiers.control && !modifiers.alt => {
                self.toggle_suggestions();
                InputDispatch::Consumed
            }
            _ => InputDispatch::Unhandled,
        }
    }

    /// Handle the fixed-key frontend-local bindings (ADR-027): modal overlay
    /// keys, selection movement and the shared command router. Character
    /// bindings (quit/help/sort/suggestions/search input) go through
    /// [`ShellApp::handle_local_char`].
    pub fn handle_local_key(&mut self, event: ShellKeyEvent) -> InputDispatch {
        if self.application.interaction.is_open() {
            if event.key == KeyCode::Escape {
                self.dismiss_overlay();
            }
            return InputDispatch::Consumed;
        }
        if self.help_open() {
            if event.key == KeyCode::Escape {
                self.toggle_help();
            }
            return InputDispatch::Consumed;
        }
        if self.suggestions_open() {
            if event.key == KeyCode::Escape {
                self.toggle_suggestions();
            }
            return InputDispatch::Consumed;
        }
        if self.search_active() {
            match event.key {
                KeyCode::Escape | KeyCode::Enter | KeyCode::Tab => {
                    self.close_search();
                }
                _ => {}
            }
            return InputDispatch::Consumed;
        }
        // List navigation keys are bare-only: every frontend documents chorded
        // variants (Ctrl+End, Shift+PageUp, …) as not wired for selection
        // movement. A chorded key falls through to the shared dispatch path
        // instead of moving the selection, so a bare-key contract cannot be
        // violated by modifier cross-feed (the TUI once jumped the cursor on
        // Ctrl+End through exactly this path).
        if event.modifiers != Modifiers::NONE {
            return self.dispatch_key(event);
        }
        match event.key {
            KeyCode::ArrowUp => {
                self.move_selection(-1);
                InputDispatch::Consumed
            }
            KeyCode::ArrowDown => {
                self.move_selection(1);
                InputDispatch::Consumed
            }
            KeyCode::PageUp => {
                self.move_selection(-10);
                InputDispatch::Consumed
            }
            KeyCode::PageDown => {
                self.move_selection(10);
                InputDispatch::Consumed
            }
            KeyCode::Home => {
                self.move_selection_to_first();
                InputDispatch::Consumed
            }
            KeyCode::End => {
                self.move_selection_to_last();
                InputDispatch::Consumed
            }
            _ => self.dispatch_key(event),
        }
    }
}
