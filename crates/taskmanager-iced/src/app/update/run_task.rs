//! Run-task dialog message reducer.

use taskmanager_application::{CommandLaunchRequest, PlatformEffect};

use super::super::{IcedApp, Message};
use crate::app::{LocalSurface, LocalSurfaceKind};

impl IcedApp {
    pub(super) fn handle_run_task_message(&mut self, message: Message) -> Option<PlatformEffect> {
        match message {
            Message::OpenRunTask => {
                self.open_local_surface(LocalSurface::RunTask);
                self.run_task.error_msg = None;
                None
            }
            Message::CloseRunTask => {
                self.dismiss_local_surface_kind(LocalSurfaceKind::RunTask);
                self.run_task.error_msg = None;
                None
            }
            Message::UpdateRunTaskCommand(command) if self.run_task_open() => {
                self.run_task.command = command;
                None
            }
            Message::SubmitRunTask if self.run_task_open() => {
                if self.run_task.command.trim().is_empty() {
                    self.run_task.error_msg =
                        Some(taskmanager_application::i18n::t("search.run_command").to_string());
                    None
                } else {
                    // The command leaves through the shared platform request
                    // path (GPUI `request_run_command` parity): the shell's
                    // effect dispatch opens the shell-ui-action session, so a
                    // queued launch is a submitted request and not a silent
                    // dismissal.
                    let request = CommandLaunchRequest {
                        command: self.run_task.command.trim().to_owned(),
                    };
                    self.dismiss_local_surface_kind(LocalSurfaceKind::RunTask);
                    self.run_task.error_msg = None;
                    self.run_task.command.clear();
                    Some(PlatformEffect::CommandLaunch(request))
                }
            }
            _ => None,
        }
    }
}
