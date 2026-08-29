//! Platform-neutral service capability glue owned by the root controller.

use super::RootView;
use crate::gpui_app::services_view;
use taskmanager_application::i18n;
use taskmanager_application::{
    RefreshRequest, ServiceControlOutcome, ServiceControlRequest, ServiceDependenciesRequest,
    ServiceLogSnapshotRequest, ServiceLogStreamRequest, ServiceUpdate,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::services::{
    ServiceAction, ServiceLogErrorKind, ServiceLogFailure, ServiceLogSnapshot, ServiceLogState,
};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::SubmissionErrorKind;

use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

impl RootView {
    /// Queue a service lifecycle action on the native adapter worker.
    pub(crate) fn request_service_action(&mut self, service_id: ServiceId, action: ServiceAction) {
        if service_id.as_str().is_empty() {
            // A read-only row can never authorize a control request; surface
            // the rejection through the same typed slot as completed outcomes.
            let request_id = self.shell.begin_service_control(service_id.clone(), action);
            let _ = self
                .shell
                .accept_service_control(request_id, &service_id, action);
            self.shell.feedback.record_service(ServiceControlOutcome {
                request_id,
                service_id,
                action,
                result: Err(FailureKind::Rejected),
            });
            return;
        }
        let request_id = self.shell.begin_service_control(service_id.clone(), action);
        let submitted_at_ms = self.service_log_now_ms;
        let result = self
            .platform
            .as_mut()
            .ok_or(FailureKind::TemporarilyUnavailable)
            .and_then(|platform| {
                platform
                    .submit_service_control(
                        ServiceControlRequest {
                            request_id,
                            service_id: service_id.clone(),
                            action,
                        },
                        submitted_at_ms,
                    )
                    .map(|_| ())
                    .map_err(|error| control_submission_failure(error.kind))
            });
        if let Err(error) = result
            && self
                .shell
                .accept_service_control(request_id, &service_id, action)
        {
            self.shell.feedback.record_service(ServiceControlOutcome {
                request_id,
                service_id,
                action,
                result: Err(error),
            });
        }
    }

    pub fn open_service_details(&mut self, service_id: ServiceId) {
        if service_id.as_str().is_empty() {
            return;
        }
        self.open_window_surface(super::window_surface::WindowSurface::ServiceDetails(
            service_id.clone(),
        ));
        if !self.service_details.select(&service_id) {
            return;
        }
        let submitted_at_ms = self.service_log_now_ms;

        let dependency_attempt = self
            .shell
            .service_dependencies
            .begin_attempt(service_id.clone());
        let dependencies = self
            .platform
            .as_mut()
            .ok_or(FailureKind::TemporarilyUnavailable)
            .and_then(|platform| {
                platform
                    .submit_service_dependencies(
                        ServiceDependenciesRequest {
                            service_id: service_id.clone(),
                        },
                        submitted_at_ms,
                    )
                    .map_err(|error| {
                        taskmanager_application::service_submission_failure(error.kind)
                    })
            });
        match dependencies {
            Ok(request_id) => self
                .shell
                .service_dependencies
                .accept_attempt(dependency_attempt, request_id),
            Err(error) => self
                .shell
                .service_dependencies
                .reject_attempt(dependency_attempt, error),
        };

        let logs = self
            .platform
            .as_mut()
            .ok_or(FailureKind::TemporarilyUnavailable)
            .and_then(|platform| {
                platform
                    .submit_service_log_snapshot(
                        ServiceLogSnapshotRequest {
                            service_id: service_id.clone(),
                        },
                        submitted_at_ms,
                    )
                    .map(|_| ())
                    .map_err(|error| {
                        taskmanager_application::service_submission_failure(error.kind)
                    })
            });
        if let Err(error) = logs {
            self.service_details
                .apply(log_failure_update(&service_id, error));
        }
    }

    pub(crate) fn refresh_service_logs(&mut self, service_id: &ServiceId) {
        if service_id.as_str().is_empty() || !self.service_details.begin_log_refresh(service_id) {
            return;
        }
        let submitted_at_ms = self.service_log_now_ms;
        let result = self
            .platform
            .as_mut()
            .ok_or(FailureKind::TemporarilyUnavailable)
            .and_then(|platform| {
                platform
                    .submit_service_log_snapshot(
                        ServiceLogSnapshotRequest {
                            service_id: service_id.clone(),
                        },
                        submitted_at_ms,
                    )
                    .map(|_| ())
                    .map_err(|error| {
                        taskmanager_application::service_submission_failure(error.kind)
                    })
            });
        if let Err(error) = result {
            self.service_details
                .apply(log_failure_update(service_id, error));
        }
    }

    /// Queue at most one incremental log request per second. This is called by
    /// the 200ms application task, never by rendering.
    pub(crate) fn poll_service_details(&mut self) -> bool {
        let mut changed = false;
        if let Some(notice) = self.service_details.poll_export() {
            self.report_service_log_export_notice(notice);
            changed = true;
        }
        let Some(service_id) = self.service_details_target().cloned() else {
            return changed;
        };
        let Some(query) = self
            .service_details
            .next_follow_request(&service_id, self.service_log_now_ms)
        else {
            return changed;
        };
        let Some(attempt_id) = self.service_details.begin_stream_attempt(query.clone()) else {
            return changed;
        };
        let submitted_at_ms = self.service_log_now_ms;
        let result = self
            .platform
            .as_mut()
            .ok_or(FailureKind::TemporarilyUnavailable)
            .and_then(|platform| {
                platform
                    .submit_service_log_stream(
                        ServiceLogStreamRequest {
                            query: query.clone(),
                        },
                        submitted_at_ms,
                    )
                    .map_err(|error| {
                        taskmanager_application::service_submission_failure(error.kind)
                    })
            });
        match result {
            Ok(request_id) => self.service_details.accept_stream(attempt_id, request_id),
            Err(error) => self.service_details.reject_stream(attempt_id, error),
        }
        true
    }

    pub(crate) fn export_service_details_logs(
        &mut self,
        service_id: &ServiceId,
        display_name: &str,
        now_micros: u64,
    ) {
        if let Some(notice) = self
            .service_details
            .export_logs(service_id, display_name, now_micros)
        {
            self.report_service_log_export_notice(notice);
        }
    }

    fn report_service_log_export_notice(&mut self, notice: services_view::ServiceLogExportNotice) {
        let (severity, lifecycle, text) = match notice {
            services_view::ServiceLogExportNotice::NothingToExport => (
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                i18n::t("svc.logs_nothing_to_export").to_owned(),
            ),
            services_view::ServiceLogExportNotice::Exporting(destination) => (
                FeedbackSeverity::Info,
                FeedbackLifecycle::UntilReplaced,
                format!(
                    "{} · {}",
                    i18n::t("svc.logs_exporting"),
                    destination.display()
                ),
            ),
            services_view::ServiceLogExportNotice::Exported(destination) => (
                FeedbackSeverity::Success,
                FeedbackLifecycle::SHORT,
                i18n::t("svc.logs_exported").replace("{path}", &destination.display().to_string()),
            ),
            services_view::ServiceLogExportNotice::Failed(error) => (
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                format!("{}: {error}", i18n::t("svc.logs_export_failed")),
            ),
        };
        self.shell
            .report_notice(FeedbackSource::Persistence, severity, lifecycle, text);
    }

    pub(super) fn apply_service_updates(&mut self, updates: Vec<ServiceUpdate>) {
        for update in updates {
            // Action outcomes are accepted and surfaced by the shared
            // projection fold; this view arm only routes log/dependency
            // updates into the service-details panel.
            match update {
                ServiceUpdate::Dependencies {
                    request_id,
                    service_id,
                    deps,
                } => {
                    self.shell
                        .service_dependencies
                        .resolve(request_id, service_id, deps);
                }
                ServiceUpdate::DependenciesUnavailable {
                    request_id,
                    service_id,
                    error,
                } => {
                    self.shell
                        .service_dependencies
                        .fail(request_id, service_id, error);
                }
                update => self.service_details.apply(update),
            }
        }
    }

    pub(super) fn apply_service_control_outcome_from_shared(
        &mut self,
        outcome: taskmanager_application::ServiceControlOutcome,
    ) {
        let succeeded = outcome.result.is_ok();
        self.shell.feedback.record_service(outcome);
        if succeeded {
            self.request_refresh(RefreshRequest::Services);
        }
    }
}

/// Fold a typed service control outcome into the action-bar copy (the single
/// outcome→文案 layer; the typed slot in `shell.feedback` stays authoritative).
pub(crate) fn service_action_feedback(
    result: Result<(), FailureKind>,
    action: ServiceAction,
    target: &str,
) -> services_view::ActionFeedback {
    super::platform_lists::control_feedback(result, service_action_label(action), target)
}

const fn control_submission_failure(kind: SubmissionErrorKind) -> FailureKind {
    match kind {
        SubmissionErrorKind::Busy => FailureKind::Rejected,
        SubmissionErrorKind::RuntimeStopped | SubmissionErrorKind::UnsupportedCapability => {
            FailureKind::TemporarilyUnavailable
        }
        SubmissionErrorKind::InvalidRequest => FailureKind::ProviderFault,
    }
}

fn log_failure_update(service_id: &ServiceId, failure: FailureKind) -> ServiceUpdate {
    let kind = ServiceLogErrorKind::from_failure(failure);
    ServiceUpdate::Logs(ServiceLogSnapshot {
        service_id: service_id.clone(),
        state: ServiceLogState::Unavailable(ServiceLogFailure::with_detail(
            kind,
            format!("service log request was rejected: {failure:?}"),
        )),
    })
}

fn service_action_label(action: ServiceAction) -> &'static str {
    match action {
        ServiceAction::Start => i18n::t("svc.start"),
        ServiceAction::Stop => i18n::t("svc.stop"),
        ServiceAction::Restart => i18n::t("svc.restart"),
        ServiceAction::Enable => i18n::t("common.enable"),
        ServiceAction::Disable => i18n::t("common.disable"),
    }
}
