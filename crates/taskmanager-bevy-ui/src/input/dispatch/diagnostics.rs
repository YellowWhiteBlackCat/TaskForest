//! The product modal owns all input ahead of page/menu/quit bindings.
use super::{DispatchFrame, KeyPress};
use crate::about_modal::AboutCommand;
use crate::first_run_modal::FirstRunCommand;
use crate::pages::system::diagnostic_modal::DiagnosticCommand;
use crate::system_information_modal::SystemInformationCommand;
use crate::window_surface::{WindowSurface, WindowSurfaceCommand};
use bevy::input::keyboard::KeyCode;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::first_run::FirstRunPhase;
use taskmanager_core::core::setup::SetupScriptAction;

impl DispatchFrame<'_, '_, '_, '_, '_, '_> {
    pub(super) fn product_modal(&mut self, press: KeyPress) -> bool {
        let Some(surface) = self.surface.and_then(|surface| surface.0.as_ref()) else {
            return false;
        };
        if press.modifiers.control || press.modifiers.alt || press.modifiers.platform {
            return true;
        }
        match surface {
            WindowSurface::ProcessTree
            | WindowSurface::EventCenter
            | WindowSurface::SavedViews
            | WindowSurface::SidebarDevices => {
                if press.key_code == KeyCode::Escape {
                    self.commands
                        .trigger(WindowSurfaceCommand::Close(surface.kind()));
                }
                return true;
            }
            WindowSurface::Diagnostic(state) => {
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
            }
            WindowSurface::About => match press.key_code {
                KeyCode::Escape => self.commands.trigger(AboutCommand::Close),
                KeyCode::KeyD => self.commands.trigger(AboutCommand::Diagnostics),
                KeyCode::KeyC => self.commands.trigger(AboutCommand::Copy),
                KeyCode::KeyR => self.commands.trigger(AboutCommand::Repository),
                KeyCode::KeyI => self.commands.trigger(AboutCommand::SystemInformation),
                _ => {}
            },
            WindowSurface::SystemInformation(_) => {
                let command = match press.key_code {
                    KeyCode::Escape => Some(SystemInformationCommand::Close),
                    KeyCode::KeyC => Some(SystemInformationCommand::Copy),
                    KeyCode::ArrowUp => Some(SystemInformationCommand::Scroll(-32.0)),
                    KeyCode::ArrowDown => Some(SystemInformationCommand::Scroll(32.0)),
                    KeyCode::PageUp => Some(SystemInformationCommand::Scroll(-200.0)),
                    KeyCode::PageDown => Some(SystemInformationCommand::Scroll(200.0)),
                    KeyCode::Home => Some(SystemInformationCommand::Scroll(-f32::MAX)),
                    KeyCode::End => Some(SystemInformationCommand::Scroll(f32::MAX)),
                    _ => None,
                };
                if let Some(command) = command {
                    self.commands.trigger(command);
                }
            }
            WindowSurface::FirstRun => {
                let state = self.setup.map(|setup| setup.0.view());
                let command = match press.key_code {
                    KeyCode::Escape => Some(FirstRunCommand::Close),
                    KeyCode::ArrowUp => Some(FirstRunCommand::Scroll(-32.0)),
                    KeyCode::ArrowDown => Some(FirstRunCommand::Scroll(32.0)),
                    KeyCode::PageUp => Some(FirstRunCommand::Scroll(-200.0)),
                    KeyCode::PageDown => Some(FirstRunCommand::Scroll(200.0)),
                    KeyCode::Home => Some(FirstRunCommand::Scroll(-f32::MAX)),
                    KeyCode::End => Some(FirstRunCommand::Scroll(f32::MAX)),
                    KeyCode::Digit1 => Some(FirstRunCommand::Copy(0)),
                    KeyCode::Digit2 => Some(FirstRunCommand::Copy(1)),
                    KeyCode::Digit3 => Some(FirstRunCommand::Copy(2)),
                    _ if state.is_some_and(|state| state.action_pending()) => None,
                    KeyCode::Enter | KeyCode::KeyV => {
                        Some(FirstRunCommand::Action(SetupScriptAction::View))
                    }
                    KeyCode::KeyR
                        if press.modifiers.shift
                            && state.is_some_and(|state| {
                                state.phase == FirstRunPhase::RestartRequired
                            }) =>
                    {
                        Some(FirstRunCommand::Action(SetupScriptAction::Restart))
                    }
                    KeyCode::KeyR if !press.modifiers.shift => {
                        Some(FirstRunCommand::Action(SetupScriptAction::Run))
                    }
                    KeyCode::KeyU => Some(FirstRunCommand::Action(SetupScriptAction::Revert)),
                    KeyCode::KeyD => Some(FirstRunCommand::Documentation),
                    KeyCode::KeyT
                        if state.is_some_and(|state| {
                            matches!(state.phase, FirstRunPhase::Failed(_))
                        }) =>
                    {
                        state
                            .and_then(|state| state.last_action)
                            .filter(|action| {
                                matches!(action, SetupScriptAction::Run | SetupScriptAction::Revert)
                            })
                            .map(FirstRunCommand::Action)
                    }
                    _ => None,
                };
                if let Some(command) = command {
                    self.commands.trigger(command);
                }
            }
        }
        true
    }
}
