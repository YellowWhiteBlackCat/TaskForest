//! `RootView` process-insights facet: non-blocking polling and correlated
//! completion of network, GPU, resource, and isolation detail for the selected
//! process, plus affinity event/failure correlation.

use crate::gpui_app::process_insights::{
    ProcessInsightsError, ProcessInsightsErrorKind, ProcessInsightsRenderState,
};
use gpui::Context;
use taskmanager_application::ProcessInsightFacet;
use taskmanager_application::{
    ProcessInsightsRevision, ProjectedProcessInsights, request_submission_failure,
};
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessLiveKey};
use taskmanager_platform_contract::SubmissionErrorKind;

use super::{ProcessDetailsSection, RootView, platform_submission_time_ms};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProcessInsightsRequest {
    target: FrozenProcessIdentity,
    revision: ProcessInsightsRevision,
}

/// One correlated request and its immutable application-owned seven-facet view.
/// Independent replies keep their own Pending/Current/Unavailable state.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) enum ProcessInsightsLifecycle {
    #[default]
    Idle,
    Loading {
        request: ProcessInsightsRequest,
    },
    Showing {
        request: ProcessInsightsRequest,
        projection: Box<ProjectedProcessInsights>,
    },
    Failed {
        target: FrozenProcessIdentity,
        error: ProcessInsightsError,
    },
}

impl ProcessInsightsLifecycle {
    fn target(&self) -> Option<&FrozenProcessIdentity> {
        match self {
            Self::Idle => None,
            Self::Loading { request } | Self::Showing { request, .. } => Some(&request.target),
            Self::Failed { target, .. } => Some(target),
        }
    }

    fn is_settled_for(&self, target: &FrozenProcessIdentity) -> bool {
        matches!(self, Self::Showing { request, projection }
            if request.target == *target && !projection.is_collecting())
            || matches!(self, Self::Failed { target: current, .. } if current == target)
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
        let Some(identity) = target.live_key() else {
            return;
        };
        *self = Self::Failed {
            target,
            error: ProcessInsightsError {
                identity: Some(identity),
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

    fn apply(&mut self, projection: ProjectedProcessInsights) -> bool {
        let request = match self {
            Self::Loading { request } | Self::Showing { request, .. } => request,
            Self::Idle | Self::Failed { .. } => return false,
        };
        if request.revision != projection.revision || request.target != projection.target {
            return false;
        }
        let request = request.clone();
        if matches!(self, Self::Showing { projection: current, .. } if current.as_ref() == &projection)
        {
            return false;
        }
        *self = Self::Showing {
            request,
            projection: Box::new(projection),
        };
        true
    }

    pub(super) fn render_state(&self) -> ProcessInsightsRenderState<'_> {
        match self {
            Self::Idle | Self::Loading { .. } => ProcessInsightsRenderState::Loading,
            Self::Showing { projection, .. } => ProcessInsightsRenderState::Projection(projection),
            Self::Failed { error, .. } => ProcessInsightsRenderState::Error(error),
        }
    }

    pub(super) fn install_capture_projection(&mut self, projection: ProjectedProcessInsights) {
        self.begin(projection.target.clone(), projection.revision);
        self.apply(projection);
    }

    pub(super) fn is_ready_for(&self, identity: ProcessLiveKey) -> bool {
        matches!(self, Self::Showing { request, projection }
            if request.target.live_key() == Some(identity)
                && projection.raw_identity().and_then(ProcessLiveKey::from_identity) == Some(identity)
                && !projection.is_collecting())
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

    pub fn open_process_details(
        &mut self,
        identity: ProcessLiveKey,
        section: ProcessDetailsSection,
    ) {
        let Some(target) = self.frozen_process(identity) else {
            return;
        };
        if self.process_insights.target() != Some(&target) {
            self.process_insights.clear();
            self.shell.close_network_escalation();
        }
        if self.process_properties_identity() != Some(identity) {
            self.details_facet = ProcessInsightFacet::Network;
            self.details_insight_offset = 0;
        }
        self.details_section = section;
        self.open_shared_process_properties(target);
    }

    pub(crate) fn select_process_insight_facet(&mut self, facet: ProcessInsightFacet) {
        self.details_facet = facet;
        self.details_insight_offset = 0;
        self.dialog_scroll
            .process_details
            .set_offset(Default::default());
    }
    pub(crate) fn page_process_insights(&mut self, first: usize) {
        self.details_insight_offset = first;
        self.dialog_scroll
            .process_details
            .set_offset(Default::default());
    }

    pub(crate) fn refresh_process_insights(&mut self) -> bool {
        if self.process_properties_target().is_none()
            || matches!(
                self.process_insights,
                ProcessInsightsLifecycle::Loading { .. }
            )
        {
            return false;
        }
        if matches!(&self.process_insights, ProcessInsightsLifecycle::Showing { projection, .. } if projection.is_collecting())
        {
            return false;
        }
        self.process_insights.clear();
        self.poll_process_insights()
    }

    /// Called from the application update task or explicit refresh. It submits a
    /// non-blocking request to the platform observation facet; collection never
    /// occurs from `Render` or the GPUI thread.
    pub(crate) fn poll_process_insights(&mut self) -> bool {
        let Some(identity) = self.process_properties_identity() else {
            return self.process_insights.clear();
        };
        let Some(target) = self.process_properties_target().cloned() else {
            return self.process_insights.clear();
        };
        if self.frozen_process(identity).is_none() {
            self.process_insights
                .fail_before_submission(target, ProcessInsightsErrorKind::ProcessUnavailable);
            return true;
        }
        if self.process_insights.is_settled_for(&target)
            || matches!(
                &self.process_insights,
                ProcessInsightsLifecycle::Loading { request } | ProcessInsightsLifecycle::Showing { request, .. } if request.target == target
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
        self.process_insights
            .begin(submission.target.clone(), submission.revision);
        self.apply_process_insights_projection(submission.projection);
        true
    }

    pub(crate) fn apply_process_insights_projection(
        &mut self,
        projection: ProjectedProcessInsights,
    ) -> bool {
        if self.process_properties_target() != Some(&projection.target) {
            return false;
        }
        if projection
            .target
            .live_key()
            .and_then(|identity| self.frozen_process(identity))
            .is_none()
        {
            self.process_insights.fail_before_submission(
                projection.target,
                ProcessInsightsErrorKind::ProcessUnavailable,
            );
            return true;
        }
        self.process_insights.apply(projection)
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_process_insights_ui_tests.rs"]
mod tests;
