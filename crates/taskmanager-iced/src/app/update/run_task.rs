//! Run-task dialog message reducer.

use taskmanager_application::{CommandLaunchRequest, PlatformEffect};

use super::super::{IcedApp, Message};
use crate::app::{LocalSurface, LocalSurfaceKind};
use taskmanager_application::i18n::t;

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
            Message::UpdateRunTaskEditor(action) if self.run_task_open() => {
                self.run_task.perform(action);
                None
            }
            Message::SubmitRunTask if self.run_task_open() => {
                let draft = self.run_task.command();
                let Some(request) = CommandLaunchRequest::from_draft(&draft) else {
                    self.run_task.error_msg = Some(t("search.run_command").to_string());
                    return None;
                };
                self.dismiss_local_surface_kind(LocalSurfaceKind::RunTask);
                self.run_task.error_msg = None;
                self.run_task.clear_command();
                Some(PlatformEffect::CommandLaunch(request))
            }
            _ => None,
        }
    }
}
