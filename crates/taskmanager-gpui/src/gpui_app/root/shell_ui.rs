//! `RootView` shell-integration facet: typed submission and correlated
//! completion of command-launch, resource-reveal, and URL-open intents, plus
//! failure feedback.

use taskmanager_application::{
    CapabilityId, CommandLaunchRequest, CorrelatedShellEvent, OperationFailure, RefreshRequest,
    RequestAttemptId, RequestId, ResourceRevealRequest, ShellUiActionIntent, ShellUiActionReceipt,
    ShellUiActionState, SubmissionErrorKind, UrlOpenRequest, request_submission_failure,
};

use crate::i18n;

use super::clipboard::process_failure_message;
use super::{RootView, platform_submission_time_ms};
use gpui::Context;

impl RootView {
    fn finish_shell_submission(
        &mut self,
        attempt: RequestAttemptId,
        result: Result<RequestId, SubmissionErrorKind>,
        cx: &mut Context<Self>,
    ) -> bool {
        match result {
            Ok(request_id) => self.shell.accept_shell_ui_action(attempt, request_id),
            Err(kind) => {
                let accepted = self
                    .shell
                    .reject_shell_ui_action(attempt, request_submission_failure(kind));
                if accepted {
                    self.apply_current_shell_failure(cx);
                }
                false
            }
        }
    }

    pub(crate) fn request_run_command(&mut self, cx: &mut Context<Self>) -> bool {
        let command = self.run_command_text(cx).trim().to_owned();
        if command.is_empty() {
            self.run_error = Some("No command given".into());
            return false;
        }
        self.run_error = None;
        let request = CommandLaunchRequest { command };
        let attempt = self
            .shell
            .begin_shell_ui_action(ShellUiActionIntent::Command(request.clone()));
        let result = self.platform.as_mut().map_or_else(
            || Err(SubmissionErrorKind::RuntimeStopped),
            |platform| {
                platform
                    .submit_command_launch(request, platform_submission_time_ms())
                    .map_err(|error| error.kind)
            },
        );
        self.finish_shell_submission(attempt, result, cx)
    }

    pub(super) fn request_reveal_process(&mut self, pid: u32, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.frozen_process(pid) else {
            self.show_local_feedback(
                format!(
                    "{}: process identity unavailable",
                    i18n::t("hint.could_not_open_location")
                ),
                cx,
            );
            return false;
        };
        let cached_executable = self
            .processes()
            .iter()
            .find(|process| process.pid == pid)
            .and_then(|process| process.current_exe_path().map(ToOwned::to_owned));
        let request = ResourceRevealRequest {
            target,
            cached_executable,
        };
        let attempt = self
            .shell
            .begin_shell_ui_action(ShellUiActionIntent::Reveal(request.clone()));
        let result = self.platform.as_mut().map_or_else(
            || Err(SubmissionErrorKind::RuntimeStopped),
            |platform| {
                platform
                    .submit_resource_reveal(request, platform_submission_time_ms())
                    .map_err(|error| error.kind)
            },
        );
        self.finish_shell_submission(attempt, result, cx)
    }

    pub(crate) fn request_open_url(&mut self, url: String, cx: &mut Context<Self>) -> bool {
        let request = UrlOpenRequest { url };
        let attempt = self
            .shell
            .begin_shell_ui_action(ShellUiActionIntent::OpenUrl(request.clone()));
        let result = self.platform.as_mut().map_or_else(
            || Err(SubmissionErrorKind::RuntimeStopped),
            |platform| {
                platform
                    .submit_url_open(request, platform_submission_time_ms())
                    .map_err(|error| error.kind)
            },
        );
        self.finish_shell_submission(attempt, result, cx)
    }

    pub(crate) fn apply_shell_event(
        &mut self,
        correlated: CorrelatedShellEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let ShellUiActionState::Ready(ready) = self.shell.shell_ui_action_state().clone() else {
            return false;
        };
        if ready.request_id != correlated.request_id {
            return false;
        }
        match (ready.intent, ready.receipt) {
            (ShellUiActionIntent::Command(_), ShellUiActionReceipt::CommandLaunched { .. }) => {
                self.dismiss_window_surface(
                    super::WindowSurfaceKind::RunTask,
                    super::WindowSurfaceDismissReason::Completed,
                );
                if let Some(input) = self.run_input.as_ref().cloned() {
                    input.update(cx, |state, cx| state.set_value("", cx));
                }
                self.request_refresh(RefreshRequest::Processes);
            }
            (
                ShellUiActionIntent::Reveal(_) | ShellUiActionIntent::OpenUrl(_),
                ShellUiActionReceipt::TargetOpened,
            ) => {}
            _ => return false,
        }
        true
    }

    pub(crate) fn apply_shell_failure(
        &mut self,
        failure: &OperationFailure,
        cx: &mut Context<Self>,
    ) -> bool {
        if failure.capability != CapabilityId::COMMAND_LAUNCH
            && failure.capability != CapabilityId::RESOURCE_REVEAL
            && failure.capability != CapabilityId::URL_OPEN
        {
            return false;
        }
        let ShellUiActionState::Failed(failed) = self.shell.shell_ui_action_state() else {
            return false;
        };
        if failed.correlation
            != taskmanager_application::RequestCorrelation::Request(failure.request_id)
            || failed.intent.capability() != failure.capability
        {
            return false;
        }
        self.apply_current_shell_failure(cx);
        true
    }

    fn apply_current_shell_failure(&mut self, cx: &mut Context<Self>) {
        let ShellUiActionState::Failed(failed) = self.shell.shell_ui_action_state().clone() else {
            return;
        };
        let error = process_failure_message(failed.failure);
        match failed.intent {
            ShellUiActionIntent::Command(_) => self.run_error = Some(error.to_string()),
            ShellUiActionIntent::Reveal(_) => {
                self.show_local_feedback(
                    format!("{}: {error}", i18n::t("hint.could_not_open_location")),
                    cx,
                );
            }
            ShellUiActionIntent::OpenUrl(_) => {
                self.show_local_feedback(format!("Could not open browser: {error}"), cx);
            }
        }
    }

    pub(super) fn close_run_command_session(&mut self) {
        let is_command = match self.shell.shell_ui_action_state() {
            ShellUiActionState::Loading { intent, .. } => {
                matches!(intent, ShellUiActionIntent::Command(_))
            }
            ShellUiActionState::Ready(ready) => {
                matches!(ready.intent, ShellUiActionIntent::Command(_))
            }
            ShellUiActionState::Failed(failed) => {
                matches!(failed.intent, ShellUiActionIntent::Command(_))
            }
            ShellUiActionState::Closed => false,
        };
        if is_command {
            self.shell.close_shell_ui_action();
        }
    }
}
