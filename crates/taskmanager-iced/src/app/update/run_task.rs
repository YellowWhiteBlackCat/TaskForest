//! Run-task dialog message reducer.

use super::super::{IcedApp, Message};
use crate::app::{LocalSurface, LocalSurfaceKind};

impl IcedApp {
    pub(super) fn handle_run_task_message(&mut self, message: Message) {
        match message {
            Message::OpenRunTask => {
                self.open_local_surface(LocalSurface::RunTask);
                self.run_task.error_msg = None;
            }
            Message::CloseRunTask => {
                self.dismiss_local_surface_kind(LocalSurfaceKind::RunTask);
                self.run_task.error_msg = None;
            }
            Message::UpdateRunTaskCommand(command) if self.run_task_open() => {
                self.run_task.command = command;
            }
            Message::ToggleRunTaskAdmin if self.run_task_open() => {
                self.run_task.as_admin = !self.run_task.as_admin;
            }
            Message::SubmitRunTask if self.run_task_open() => {
                if self.run_task.command.trim().is_empty() {
                    self.run_task.error_msg =
                        Some(taskmanager_application::i18n::t("search.run_command").to_string());
                } else {
                    self.dismiss_local_surface_kind(LocalSurfaceKind::RunTask);
                    self.run_task.error_msg = None;
                    self.run_task.command.clear();
                }
            }
            _ => {}
        }
    }
}
