//! Diagnostic review input owns the press before navigation or controls.

use super::{DispatchFrame, KeyPress};
use crate::pages::system::diagnostic_modal::DiagnosticCommand;
use bevy::input::keyboard::KeyCode;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;

impl DispatchFrame<'_, '_, '_, '_, '_, '_> {
    pub(super) fn diagnostic_modal(&mut self, press: KeyPress) -> bool {
        let Some(state) = self.diagnostic.and_then(|modal| modal.0.as_ref()) else {
            return false;
        };
        let command = match press.key_code {
            KeyCode::Escape => Some(DiagnosticCommand::Close),
            KeyCode::ArrowUp => Some(DiagnosticCommand::Scroll(-32.0)),
            KeyCode::ArrowDown => Some(DiagnosticCommand::Scroll(32.0)),
            KeyCode::PageUp => Some(DiagnosticCommand::Scroll(-200.0)),
            KeyCode::PageDown => Some(DiagnosticCommand::Scroll(200.0)),
            KeyCode::Home => Some(DiagnosticCommand::Scroll(-f32::MAX)),
            KeyCode::End => Some(DiagnosticCommand::Scroll(f32::MAX)),
            KeyCode::Enter if matches!(state, DiagnosticBundleUiState::Failed(_)) => {
                Some(DiagnosticCommand::Retry)
            }
            KeyCode::Enter => Some(DiagnosticCommand::Confirm),
            _ => None,
        };
        if let Some(command) = command {
            self.commands.trigger(command);
        }
        true
    }
}
