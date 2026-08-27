//! `RootView` process-insights facet: non-blocking polling and correlated
//! completion of network, GPU, resource, and isolation detail for the selected
//! process, plus affinity event/failure correlation.

use crate::gpui_app::process_insights::{
    ProcessInsightsError, ProcessInsightsErrorKind, ProcessInsightsRenderState,
    ProcessInsightsState, state_from_snapshot,
};
use gpui::Context;
use taskmanager_application::{
    FailureKind, FrozenProcessIdentity, ProcessInsightFacetState, ProcessInsightUnavailable,
    ProcessInsightsRevision, ProjectedProcessInsights, SubmissionErrorKind,
    request_submission_failure,
};

use super::{ProcessDetailsSection, RootView, platform_submission_time_ms};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProcessInsightsRequest {
    target: FrozenProcessIdentity,
    revision: ProcessInsightsRevision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ProcessInsightsAttempt {
    BeforeSubmission(FrozenProcessIdentity),
    Submitted(ProcessInsightsRequest),
}

impl ProcessInsightsAttempt {
    const fn target(&self) -> &FrozenProcessIdentity {
        match self {
            Self::BeforeSubmission(target) => target,
            Self::Submitted(request) => &request.target,
        }
    }
}

/// Window-local adapter around the application-correlated process-insight
/// projection. Request identity, exact frozen target and terminal payload are
/// one state: an idle/terminal phase cannot accidentally retain a pending
/// tuple, and only the matching loading request accepts a projection.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) enum ProcessInsightsLifecycle {
    #[default]
    Idle,
    Loading {
        request: ProcessInsightsRequest,
    },
    Ready {
        request: ProcessInsightsRequest,
        snapshot: Box<taskmanager_application::ProcessTelemetrySnapshot>,
    },
    Failed {
        attempt: ProcessInsightsAttempt,
        error: ProcessInsightsError,
    },
}

impl ProcessInsightsLifecycle {
    fn target(&self) -> Option<&FrozenProcessIdentity> {
        match self {
            Self::Idle => None,
            Self::Loading { request } | Self::Ready { request, .. } => Some(&request.target),
            Self::Failed { attempt, .. } => Some(attempt.target()),
        }
    }

    fn is_settled_for(&self, target: &FrozenProcessIdentity) -> bool {
        matches!(
            self,
            Self::Ready { request, .. } if request.target == *target
        ) || matches!(
            self,
            Self::Failed { attempt, .. } if attempt.target() == target
        )
    }

    pub(super) fn clear(&mut self) -> bool {
        if matches!(self, Self::Idle) {
            return false;
        }
        *self = Self::Idle;
        true
    }

    fn fail_before_submission(
        &mut self,
        target: FrozenProcessIdentity,
        kind: ProcessInsightsErrorKind,
    ) {
        let pid = target.pid;
        *self = Self::Failed {
            attempt: ProcessInsightsAttempt::BeforeSubmission(target),
            error: ProcessInsightsError {
                pid,
                kind,
                last_success_ms: None,
            },
        };
    }

    fn begin(&mut self, target: FrozenProcessIdentity, revision: ProcessInsightsRevision) {
        *self = Self::Loading {
            request: ProcessInsightsRequest { target, revision },
        };
    }

    fn fail_submitted(&mut self, kind: ProcessInsightsErrorKind) -> bool {
        let Self::Loading { request } = self else {
            return false;
        };
        let request = request.clone();
        let pid = request.target.pid;
        *self = Self::Failed {
            attempt: ProcessInsightsAttempt::Submitted(request),
            error: ProcessInsightsError {
                pid,
                kind,
                last_success_ms: None,
            },
        };
        true
    }

    fn apply(&mut self, projection: ProjectedProcessInsights) -> bool {
        let Self::Loading { request } = self else {
            return false;
        };
        if request.revision != projection.revision || request.target != projection.target {
            return false;
        }
        let request = request.clone();
        if let Some(snapshot) = projection.complete_snapshot() {
            *self = match state_from_snapshot(snapshot) {
                ProcessInsightsState::Ready(snapshot) => Self::Ready { request, snapshot },
                ProcessInsightsState::Error(error) => Self::Failed {
                    attempt: ProcessInsightsAttempt::Submitted(request),
                    error,
                },
                ProcessInsightsState::Loading { .. } => return false,
            };
            return true;
        }
        if projection_has_pending(&projection) {
            return true;
        }
        let kind = projection_error_kind(&projection);
        self.fail_submitted(kind)
    }

    pub(super) fn render_state(&self) -> ProcessInsightsRenderState<'_> {
        match self {
            Self::Idle => ProcessInsightsRenderState::Loading,
            Self::Loading { .. } => ProcessInsightsRenderState::Loading,
            Self::Ready { snapshot, .. } => ProcessInsightsRenderState::Ready(snapshot),
            Self::Failed { error, .. } => ProcessInsightsRenderState::Error(error),
        }
    }

    pub(super) fn install_capture_state(
        &mut self,
        target: FrozenProcessIdentity,
        state: ProcessInsightsState,
    ) {
        let request = ProcessInsightsRequest {
            target: target.clone(),
            revision: ProcessInsightsRevision::new(u64::MAX),
        };
        *self = match state {
            ProcessInsightsState::Loading { .. } => Self::Loading { request },
            ProcessInsightsState::Ready(snapshot) => Self::Ready { request, snapshot },
            ProcessInsightsState::Error(error) => Self::Failed {
                attempt: ProcessInsightsAttempt::BeforeSubmission(target),
                error,
            },
        };
    }

    pub(super) fn is_ready_for(&self, pid: u32) -> bool {
        matches!(
            self,
            Self::Ready { request, snapshot }
                if request.target.pid == pid && snapshot.identity.pid == pid
        )
    }
}

impl RootView {
    /// User clicked "Enable per-process network" on a Process Properties
    /// network card whose traffic state is the escalatable
    /// `RequiresEscalation` denial: offer the OS-native prompt through the
    /// system-level escalation capability. The outcome surfaces as the
    /// correlated `NetworkCaptureEscalated` event; the next network
    /// observation reflects the granted capture.
    pub(crate) fn request_process_network_escalation(&mut self, cx: &mut Context<Self>) {
        let attempt = self.shell.begin_network_escalation();
        let result = self.platform.as_mut().map_or_else(
            || Err(SubmissionErrorKind::RuntimeStopped),
            |platform| {
                platform
                    .submit_process_network_escalation(platform_submission_time_ms())
                    .map_err(|error| error.kind)
            },
        );
        match result {
            Ok(request_id) => {
                self.shell.accept_network_escalation(attempt, request_id);
            }
            Err(kind) => {
                self.shell
                    .reject_network_escalation(attempt, request_submission_failure(kind));
            }
        }
        cx.notify();
    }

    pub fn open_process_details(&mut self, pid: u32, section: ProcessDetailsSection) {
        let Some(target) = self.frozen_process(pid) else {
            return;
        };
        if self.process_insights.target() != Some(&target) {
            self.process_insights.clear();
            self.shell.close_network_escalation();
        }
        self.details_section = section;
        self.open_shared_process_properties(target);
    }

    /// Called exclusively from the 200ms application update task. It submits a
    /// non-blocking request to the platform observation facet; collection never
    /// occurs from `Render` or the GPUI thread.
    pub(crate) fn poll_process_insights(&mut self) -> bool {
        let Some(pid) = self.process_properties_pid() else {
            return self.process_insights.clear();
        };
        let Some(target) = self.frozen_process(pid) else {
            // Keep a real target only when the application interaction still
            // owns one. If the row disappeared before we could freeze it,
            // retain the exact target from the open surface for diagnostics.
            let Some(target) = self.process_properties_target().cloned() else {
                return self.process_insights.clear();
            };
            self.process_insights
                .fail_before_submission(target, ProcessInsightsErrorKind::ProcessUnavailable);
            return true;
        };
        if self.process_insights.is_settled_for(&target)
            || matches!(
                &self.process_insights,
                ProcessInsightsLifecycle::Loading { request } if request.target == target
            )
        {
            return false;
        }
        let Some(platform) = self.platform.as_mut() else {
            self.process_insights
                .fail_before_submission(target, ProcessInsightsErrorKind::WorkerDisconnected);
            return true;
        };
        let submission = match platform
            .submit_process_insights(target.clone(), platform_submission_time_ms())
        {
            Ok(submission) => submission,
            Err(_) => {
                self.process_insights
                    .fail_before_submission(target, ProcessInsightsErrorKind::WorkerDisconnected);
                return true;
            }
        };
        let has_pending_requests = submission.has_pending_requests();
        let first_error = submission
            .first_error()
            .map(|error| process_insights_submission_error(error.kind));
        self.process_insights
            .begin(submission.target.clone(), submission.revision);
        if !self.apply_process_insights_projection(submission.projection) {
            let kind = first_error.unwrap_or(ProcessInsightsErrorKind::WorkerDisconnected);
            if !has_pending_requests {
                let _ = self.process_insights.fail_submitted(kind);
            }
        }
        true
    }

    pub(crate) fn apply_process_insights_projection(
        &mut self,
        projection: ProjectedProcessInsights,
    ) -> bool {
        self.process_insights.apply(projection)
    }
}

fn projection_has_pending(projection: &ProjectedProcessInsights) -> bool {
    matches!(projection.network, ProcessInsightFacetState::Pending)
        || matches!(projection.gpu, ProcessInsightFacetState::Pending)
        || matches!(projection.resources, ProcessInsightFacetState::Pending)
        || matches!(projection.isolation, ProcessInsightFacetState::Pending)
        || matches!(projection.threads, ProcessInsightFacetState::Pending)
        || matches!(projection.open_files, ProcessInsightFacetState::Pending)
}

fn projection_error_kind(projection: &ProjectedProcessInsights) -> ProcessInsightsErrorKind {
    [
        unavailable_kind(&projection.network),
        unavailable_kind(&projection.gpu),
        unavailable_kind(&projection.resources),
        unavailable_kind(&projection.isolation),
    ]
    .into_iter()
    .flatten()
    .max_by_key(|kind| process_insights_error_priority(*kind))
    .unwrap_or(ProcessInsightsErrorKind::ProcessUnavailable)
}

fn unavailable_kind<T>(state: &ProcessInsightFacetState<T>) -> Option<ProcessInsightsErrorKind> {
    let ProcessInsightFacetState::Unavailable(reason) = state else {
        return None;
    };
    Some(match reason {
        ProcessInsightUnavailable::Submission(kind) => process_insights_submission_error(*kind),
        ProcessInsightUnavailable::Provider(
            FailureKind::PermissionDenied | FailureKind::RequiresEscalation,
        ) => ProcessInsightsErrorKind::PermissionDenied,
        ProcessInsightUnavailable::Provider(FailureKind::Unsupported) => {
            ProcessInsightsErrorKind::Unsupported
        }
        ProcessInsightUnavailable::Provider(FailureKind::MissingDependency) => {
            ProcessInsightsErrorKind::ProviderUnavailable
        }
        ProcessInsightUnavailable::Provider(
            FailureKind::TimedOut
            | FailureKind::IdentityChanged
            | FailureKind::TemporarilyUnavailable
            | FailureKind::Rejected
            | FailureKind::ProviderFault,
        ) => ProcessInsightsErrorKind::ProcessUnavailable,
    })
}

const fn process_insights_error_priority(kind: ProcessInsightsErrorKind) -> u8 {
    match kind {
        ProcessInsightsErrorKind::PermissionDenied => 5,
        ProcessInsightsErrorKind::ProviderUnavailable => 4,
        ProcessInsightsErrorKind::ProcessUnavailable => 3,
        ProcessInsightsErrorKind::Unsupported => 2,
        ProcessInsightsErrorKind::WorkerDisconnected => 1,
    }
}

fn process_insights_submission_error(kind: SubmissionErrorKind) -> ProcessInsightsErrorKind {
    match kind {
        // The previous worker boundary exposed all enqueue failures as a
        // disconnected worker. Preserve that visible state while matching the
        // new typed submission vocabulary exhaustively.
        SubmissionErrorKind::Busy
        | SubmissionErrorKind::RuntimeStopped
        | SubmissionErrorKind::InvalidRequest
        | SubmissionErrorKind::UnsupportedCapability => {
            ProcessInsightsErrorKind::WorkerDisconnected
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_process_insights_ui_tests.rs"]
mod tests;
