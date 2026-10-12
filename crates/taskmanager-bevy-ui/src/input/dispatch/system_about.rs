//! Map the shared system-about command to Bevy's mounted system-information surface.

use super::{DispatchFrame, KeyPress, KeyboardOwner, dispatch};
use crate::input_contract::{InputModifiers, normalize_key, shared_key};
use crate::system_information_modal::SystemInformationCommand;
use taskmanager_application::{AppAction, CommandContext, CommandScope};
use taskmanager_shell::{InputDispatch, ShellKeyEvent};

impl DispatchFrame<'_, '_, '_, '_, '_, '_> {
    /// Render the shared `ShowSystemAbout` action through the system-info
    /// window surface after the shell command table accepts the same key.
    pub(super) fn system_about(&mut self, press: KeyPress) -> bool {
        if !matches!(press.context, KeyboardOwner::Free) {
            return false;
        }
        let modifiers = InputModifiers {
            control: press.modifiers.control,
            alt: press.modifiers.alt,
            shift: press.modifiers.shift,
            platform: press.modifiers.platform,
        };
        if !matches!(
            normalize_key(
                press.key_code,
                modifiers,
                CommandContext {
                    scope: CommandScope::Global,
                    ..CommandContext::default()
                },
            ),
            Some(AppAction::OpenSystemAbout)
        ) {
            return false;
        }
        let Some(key) = shared_key(press.key_code) else {
            return false;
        };
        let outcome = self
            .shell
            .handle_local_key(ShellKeyEvent::new(key, press.modifiers));
        if matches!(&outcome, InputDispatch::Unhandled) {
            return false;
        }
        let _ = dispatch(outcome, self.pending);
        self.shell.clear_feedback_notice();
        self.commands.trigger(SystemInformationCommand::Open);
        self.applied = true;
        true
    }
}
