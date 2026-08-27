//! Platform-neutral startup and login-session control glue.

use super::{RootView, platform_submission_time_ms};
use crate::gpui_app::list_view::ActionFeedback;
use crate::i18n;
use gpui::Context;
use taskmanager_application::{
    FailureKind, RefreshRequest, SessionControlAction, SessionControlOutcome,
    SessionControlRequest, SessionControlTarget, StartupControlOutcome, StartupControlRequest,
    SubmissionErrorKind,
};

impl RootView {
    pub(crate) fn submit_startup_control_request(&mut self, request: StartupControlRequest) {
        let request_id = request.request_id;
        let enabled = request.enabled;
        let target_name = request.entry.name.clone();
        let target_id = request.entry.id.clone();
        let queued = self
            .platform
            .as_mut()
            .ok_or(FailureKind::TemporarilyUnavailable)
            .and_then(|platform| {
                platform
                    .submit_startup_control(request, platform_submission_time_ms())
                    .map(|_| ())
                    .map_err(|error| submission_error_kind(error.kind))
            });
        if let Err(error) = queued
            && self.shell.accept_startup_control(request_id)
        {
            // Rejected submissions surface through the SAME typed slot as
            // completed outcomes — the action bar folds one typed shape.
            self.shell.feedback.record_startup(StartupControlOutcome {
                request_id,
                target_id,
                target_name,
                enabled,
                result: Err(error),
            });
        }
    }

    /// Queue a login-session action without invoking native tools on the UI thread.
    pub(crate) fn request_session_control(
        &mut self,
        session_id: String,
        action: SessionControlAction,
    ) {
        let request_id = self.shell.begin_session_control();
        self.submit_session_control_target(SessionControlTarget {
            request_id,
            session_id: session_id.clone().into(),
            action,
        });
    }

    pub(crate) fn submit_session_control_target(&mut self, target: SessionControlTarget) {
        let request_id = target.request_id;
        let session_id = target.session_id;
        let action = target.action;
        let request = SessionControlRequest {
            request_id,
            session_id: session_id.clone(),
            action,
        };
        let queued = self
            .platform
            .as_mut()
            .ok_or(FailureKind::TemporarilyUnavailable)
            .and_then(|platform| {
                platform
                    .submit_session_control(request, platform_submission_time_ms())
                    .map(|_| ())
                    .map_err(|error| submission_error_kind(error.kind))
            });
        if let Err(error) = queued
            && self.shell.accept_session_control(request_id)
        {
            self.shell.feedback.record_session(SessionControlOutcome {
                request_id,
                session_id,
                action,
                result: Err(error),
            });
        }
    }

    pub(super) fn apply_startup_outcome_from_shared(&mut self, outcome: StartupControlOutcome) {
        let succeeded = outcome.result.is_ok();
        self.shell.feedback.record_startup(outcome);
        if succeeded {
            self.request_refresh(RefreshRequest::Startup);
        }
    }

    pub(super) fn apply_session_outcome_from_shared(
        &mut self,
        outcome: SessionControlOutcome,
        cx: &mut Context<Self>,
    ) {
        let succeeded = outcome.result.is_ok();
        self.shell.feedback.record_session(outcome.clone());
        if succeeded && outcome.action == SessionControlAction::Disconnect {
            // Re-scan sessions so the terminated session disappears (the
            // deferred re-scans mirror the old accepted-outcome path).
            self.request_refresh(RefreshRequest::Sessions);
            let root = cx.entity();
            cx.spawn(async move |_this, cx| {
                gpui::Timer::after(std::time::Duration::from_millis(600)).await;
                let _ = root.update(cx, |v, _cx| v.request_refresh(RefreshRequest::Sessions));
                gpui::Timer::after(std::time::Duration::from_millis(1200)).await;
                let _ = root.update(cx, |v, _cx| v.request_refresh(RefreshRequest::Sessions));
            })
            .detach();
        }
    }
}

const fn submission_error_kind(kind: SubmissionErrorKind) -> FailureKind {
    match kind {
        SubmissionErrorKind::Busy => FailureKind::Rejected,
        SubmissionErrorKind::RuntimeStopped | SubmissionErrorKind::UnsupportedCapability => {
            FailureKind::TemporarilyUnavailable
        }
        SubmissionErrorKind::InvalidRequest => FailureKind::ProviderFault,
    }
}

pub(crate) fn control_feedback(
    result: Result<(), FailureKind>,
    action: &'static str,
    target: &str,
) -> ActionFeedback {
    let display_result = result.map_err(|kind| control_error_detail(kind).to_string());
    ActionFeedback::from_result(&display_result, action, target)
}

/// Fold a typed startup control outcome into the action-bar copy (the single
/// outcome→文案 layer; the typed slot in `shell.feedback` stays authoritative).
pub(crate) fn startup_outcome_feedback(
    result: Result<(), FailureKind>,
    enabled: bool,
    target: &str,
) -> ActionFeedback {
    control_feedback(result, startup_action_label(enabled), target)
}

/// Fold a typed login-session control outcome into the action-bar copy (see
/// [`startup_outcome_feedback`]).
pub(crate) fn session_outcome_feedback(
    result: Result<(), FailureKind>,
    action: SessionControlAction,
    session_id: &str,
) -> ActionFeedback {
    control_feedback(
        result,
        session_action_label(action),
        &session_target(session_id),
    )
}

fn startup_action_label(enabled: bool) -> &'static str {
    if enabled {
        i18n::t("common.enable")
    } else {
        i18n::t("common.disable")
    }
}

fn session_action_label(action: SessionControlAction) -> &'static str {
    match action {
        SessionControlAction::Disconnect => i18n::t("users.disconnect"),
        SessionControlAction::Lock => i18n::t("users.lock"),
    }
}

fn session_target(session_id: &str) -> String {
    format!("{} {session_id}", i18n::t("users.session_word"))
}

pub(crate) fn control_error_detail(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::Unsupported => i18n::t("feedback.unsupported"),
        FailureKind::TemporarilyUnavailable => i18n::t("feedback.provider_unavailable"),
        // RequiresEscalation is an escalatable denial; fold into the denial key.
        FailureKind::PermissionDenied | FailureKind::RequiresEscalation => {
            i18n::t("feedback.permission_denied")
        }
        FailureKind::MissingDependency => i18n::t("feedback.provider_unavailable"),
        FailureKind::TimedOut => i18n::t("feedback.timed_out"),
        FailureKind::Rejected => i18n::t("feedback.request_rejected"),
        FailureKind::IdentityChanged => i18n::t("feedback.target_changed"),
        FailureKind::ProviderFault => i18n::t("feedback.provider_failed"),
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_platform_lists_tests.rs"]
mod tests;
