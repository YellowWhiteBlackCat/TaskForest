//! Optional setup adapters over the application-owned request lifecycle.

use super::super::{FirstRunMessage, IcedApp, LocalSurface, LocalSurfaceKind, Message};
use super::dispatch::UpdateDispatch;
use taskmanager_application::first_run::{DOCUMENTATION_URL, FirstRunCompletion};
use taskmanager_application::{PlatformEffect, UrlOpenRequest};
use taskmanager_shell::QuitReason;

impl IcedApp {
    pub(crate) fn begin_first_run_observation(&mut self) {
        self.first_run.observe(
            self.runtime.platform_mut(),
            super::super::refresh::unix_now_ms(),
        );
    }

    pub(super) fn reduce_first_run_message(&mut self, message: Message) -> UpdateDispatch {
        let Message::FirstRun(intent) = message else {
            return UpdateDispatch::none();
        };
        if matches!(intent, FirstRunMessage::Open) {
            if self.first_run.view().info.is_some() {
                self.open_local_surface(LocalSurface::FirstRun);
            }
            return UpdateDispatch::none();
        }
        if self.local_surface_kind() != Some(LocalSurfaceKind::FirstRun) {
            return UpdateDispatch::none();
        }
        match intent {
            FirstRunMessage::Open => UpdateDispatch::none(),
            FirstRunMessage::Close => {
                self.dismiss_local_surface_kind(LocalSurfaceKind::FirstRun);
                UpdateDispatch::none()
            }
            FirstRunMessage::RequestAction(action) => {
                self.first_run.request(
                    action,
                    self.runtime.platform_mut(),
                    super::super::refresh::unix_now_ms(),
                );
                UpdateDispatch::none()
            }
            FirstRunMessage::OpenDocumentation => {
                UpdateDispatch::effect(Some(PlatformEffect::OpenUrl(UrlOpenRequest {
                    url: DOCUMENTATION_URL.into(),
                })))
            }
        }
    }

    pub(crate) fn apply_first_run_completion(&mut self, outcome: FirstRunCompletion) {
        if outcome == FirstRunCompletion::Restart {
            self.shell.request_quit(QuitReason::Restart);
        }
        if outcome != FirstRunCompletion::Unchanged && self.first_run.view().info.is_none() {
            self.dismiss_local_surface_kind(LocalSurfaceKind::FirstRun);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/first_run_wiring_tests.rs"]
mod tests;
