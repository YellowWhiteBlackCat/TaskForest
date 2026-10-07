//! TUI adapter for current-window PNG capture intent and app-host session.
//!
//! The capture mechanism is toolkit-agnostic (the platform adapter captures
//! the active compositor window — for a terminal product that is the terminal
//! emulator window showing the frame). The TUI owns only the request session
//! and renders its feedback through the shared notice line.

use std::path::PathBuf;

use taskmanager_app_host::WindowCaptureClient;
use taskmanager_application::i18n::t;
use taskmanager_application::window_capture::{
    WindowCaptureSession, WindowCaptureState, WindowCaptureSubmitError, WindowCaptureTarget,
};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use super::TuiApp;

#[derive(Debug, Default)]
pub(crate) enum TuiWindowCaptureRuntime {
    #[default]
    Unavailable,
    Active(WindowCaptureSession<WindowCaptureClient>),
}

impl TuiWindowCaptureRuntime {
    fn install(&mut self, client: WindowCaptureClient) {
        *self = Self::Active(WindowCaptureSession::new(client));
    }

    fn active_mut(&mut self) -> Option<&mut WindowCaptureSession<WindowCaptureClient>> {
        match self {
            Self::Unavailable => None,
            Self::Active(session) => Some(session),
        }
    }
}

impl TuiApp {
    pub(crate) fn install_window_capture_client(&mut self, client: WindowCaptureClient) {
        self.window_capture.install(client);
    }

    /// Request one current-window PNG capture. Returns whether a request was
    /// submitted; the honest unavailable/busy/failed states are reported
    /// through the shared notice line.
    pub(crate) fn request_current_window_capture(&mut self) -> bool {
        let target = std::env::var_os("TM_CAPTURE_WINDOW_OUTPUT")
            .map(PathBuf::from)
            .map(WindowCaptureTarget::path)
            .unwrap_or_else(|| WindowCaptureTarget::current_directory("taskforest-window.png"));
        let Some(session) = self.window_capture.active_mut() else {
            self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::TIMED_LONG,
                t("window_capture.unavailable"),
            );
            return false;
        };
        match session.submit(target) {
            Ok(_) => {
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::TIMED_SHORT,
                    t("window_capture.queued"),
                );
                true
            }
            Err(WindowCaptureSubmitError::Busy(_)) => {
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Warning,
                    FeedbackLifecycle::TIMED_SHORT,
                    t("window_capture.busy"),
                );
                false
            }
            Err(WindowCaptureSubmitError::RequestSpaceExhausted) => {
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::TIMED_LONG,
                    t("window_capture.unavailable"),
                );
                false
            }
            Err(WindowCaptureSubmitError::Rejected(error)) => {
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::TIMED_LONG,
                    t("window_capture.failed").replace("{}", error.detail()),
                );
                false
            }
        }
    }

    /// Drain finished captures into the shared notice line. Returns whether the
    /// frame should be redrawn.
    pub(crate) fn drain_window_capture_completions(&mut self) -> bool {
        let Some(session) = self.window_capture.active_mut() else {
            return false;
        };
        if session.drain() == 0 {
            return false;
        }
        let state = session.state().clone();
        match state {
            WindowCaptureState::Ready {
                destination,
                width,
                height,
                ..
            } => {
                let message = t("window_capture.success")
                    .replacen("{}", destination.as_ref(), 1)
                    .replacen("{}", &width.to_string(), 1)
                    .replacen("{}", &height.to_string(), 1);
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::TIMED_SHORT,
                    message,
                );
            }
            WindowCaptureState::Failed { error, .. } => {
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::TIMED_LONG,
                    t("window_capture.failed").replace("{}", error.detail()),
                );
            }
            WindowCaptureState::Closed
            | WindowCaptureState::Queued(_)
            | WindowCaptureState::Running(_) => return false,
        }
        true
    }
}

#[cfg(test)]
#[path = "../tests/headless/window_capture_tests.rs"]
mod tests;
