//! `RootView` process-control facet: typed submission and correlated completion
//! of process control, affinity, and batch intents, plus failure feedback.

use gpui::Context;

use crate::core::process::FrozenProcessIdentity;
use taskmanager_application::{
    FailureKind, ProcessAffinityControlRequest, ProcessAffinityRequest, ProcessControlRequest,
    SubmissionError, SubmissionErrorKind,
};
use taskmanager_shell::ProcessControlKind;

use super::{
    ProcessControlAction, RootView, platform_submission_time_ms, process_control_feedback,
};

impl RootView {
    pub(crate) fn accept_shared_process_control_feedback(
        &mut self,
        feedback: &taskmanager_shell::ProcessControlFeedback,
    ) {
        let (text, succeeded) = taskmanager_shell::process_control_notice_text(feedback);
        self.report_process_control_notice(text, succeeded);
    }

    pub fn record_process_control_result(
        &mut self,
        action: ProcessControlAction,
        pid: u32,
        result: Result<(), FailureKind>,
        cx: &mut Context<Self>,
    ) {
        let succeeded = result.is_ok();
        self.report_process_control_notice(
            process_control_feedback(action, pid, result),
            succeeded,
        );
        cx.notify();
    }

    pub(crate) fn report_process_control_notice(&mut self, text: String, succeeded: bool) {
        self.local_feedback_toast = None;
        self.local_feedback_subscription = None;
        self.shell.report_notice(
            taskmanager_shell::FeedbackSource::Control,
            if succeeded {
                taskmanager_shell::FeedbackSeverity::Success
            } else {
                taskmanager_shell::FeedbackSeverity::Error
            },
            if succeeded {
                taskmanager_shell::FeedbackLifecycle::SHORT
            } else {
                taskmanager_shell::FeedbackLifecycle::UntilReplaced
            },
            text,
        );
    }

    pub(crate) fn frozen_process(&self, pid: u32) -> Option<FrozenProcessIdentity> {
        self.processes()
            .iter()
            .find(|process| process.pid == pid)
            .and_then(FrozenProcessIdentity::from_process)
    }

    pub(crate) fn submit_process_control(
        &mut self,
        request: ProcessControlRequest,
        action: ProcessControlAction,
        pid: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        let result = self.platform.as_mut().map_or_else(
            || Err(FailureKind::TemporarilyUnavailable),
            |platform| {
                let request = request.clone();
                platform
                    .submit_process_control(request, platform_submission_time_ms())
                    .map_err(submission_failure_kind)
            },
        );
        match result {
            Ok(request_id) => {
                match &request {
                    ProcessControlRequest::EndTask(target) => {
                        self.shell.begin_process_control(
                            request_id,
                            target.clone(),
                            ProcessControlKind::EndTask,
                        );
                    }
                    ProcessControlRequest::SendSignal { target, signal } => {
                        self.shell.begin_process_control(
                            request_id,
                            target.clone(),
                            ProcessControlKind::Signal(*signal),
                        );
                    }
                    ProcessControlRequest::Suspend { target } => {
                        self.shell.begin_process_control(
                            request_id,
                            target.clone(),
                            ProcessControlKind::Suspend,
                        );
                    }
                    ProcessControlRequest::Resume { target } => {
                        self.shell.begin_process_control(
                            request_id,
                            target.clone(),
                            ProcessControlKind::Resume,
                        );
                    }
                    ProcessControlRequest::ExecuteBatch(_) => {}
                }
                true
            }
            Err(kind) => {
                self.record_process_control_result(action, pid, Err(kind), cx);
                false
            }
        }
    }

    pub(crate) fn request_process_affinity(&mut self, pid: u32, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.frozen_process(pid) else {
            return false;
        };
        let attempt = self.shell.begin_process_affinity_read(target.clone());
        let result = self.platform.as_mut().map_or_else(
            || Err(FailureKind::TemporarilyUnavailable),
            |platform| {
                platform
                    .submit_process_affinity(
                        ProcessAffinityRequest { target },
                        platform_submission_time_ms(),
                    )
                    .map_err(submission_failure_kind)
            },
        );
        match result {
            Ok(request_id) => {
                self.shell.accept_process_affinity_read(attempt, request_id);
                true
            }
            Err(kind) => {
                self.shell.reject_process_affinity_read(attempt, kind);
                cx.notify();
                false
            }
        }
    }

    pub(crate) fn submit_process_affinity(
        &mut self,
        pid: u32,
        cpus: Vec<u32>,
        cx: &mut Context<Self>,
    ) -> bool {
        let taskmanager_application::ProcessAffinityState::Ready(ready) =
            self.shell.process_affinity_state().clone()
        else {
            self.record_process_control_result(
                ProcessControlAction::SetAffinity,
                pid,
                Err(FailureKind::TemporarilyUnavailable),
                cx,
            );
            return false;
        };
        let target = ready.target;
        if self.frozen_process(pid).as_ref() != Some(&target) {
            self.record_process_control_result(
                ProcessControlAction::SetAffinity,
                pid,
                Err(FailureKind::IdentityChanged),
                cx,
            );
            return false;
        }
        let result = self.platform.as_mut().map_or_else(
            || Err(FailureKind::TemporarilyUnavailable),
            |platform| {
                let target_for_request = target.clone();
                let cpus_for_request = cpus.clone();
                platform
                    .submit_process_affinity_control(
                        ProcessAffinityControlRequest {
                            target: target_for_request,
                            cpus: cpus_for_request,
                        },
                        platform_submission_time_ms(),
                    )
                    .map_err(submission_failure_kind)
            },
        );
        match result {
            Ok(request_id) => {
                self.shell.begin_process_control(
                    request_id,
                    target.clone(),
                    ProcessControlKind::Affinity(cpus.clone()),
                );
                true
            }
            Err(kind) => {
                self.record_process_control_result(
                    ProcessControlAction::SetAffinity,
                    pid,
                    Err(kind),
                    cx,
                );
                false
            }
        }
    }
}

pub(super) fn submission_failure_kind(error: SubmissionError) -> FailureKind {
    match error.kind {
        // Submission failures previously shared one provider-unavailable
        // presentation. Keep that visible contract while consuming the typed
        // transport error directly instead of stringifying and reclassifying it.
        SubmissionErrorKind::Busy
        | SubmissionErrorKind::RuntimeStopped
        | SubmissionErrorKind::InvalidRequest
        | SubmissionErrorKind::UnsupportedCapability => FailureKind::TemporarilyUnavailable,
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_process_control_tests.rs"]
mod tests;
