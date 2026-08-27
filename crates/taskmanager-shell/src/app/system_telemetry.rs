//! Shared mapping of the application-owned system telemetry acceptance policy.

use taskmanager_application::{
    CorrelatedSystemTelemetryOutcome, DeviceLifecycleDiagnosticHistory, DeviceLifecycleProjection,
    ProjectedSystemTelemetry, ProjectionAcceptance,
};

use super::{FrameCommit, SystemSnapshot};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SystemTelemetryApply {
    #[default]
    Rejected,
    AcceptedPartial,
    Committed,
}

impl SystemTelemetryApply {
    #[must_use]
    pub(super) const fn is_accepted(self) -> bool {
        !matches!(self, Self::Rejected)
    }

    #[must_use]
    pub(super) const fn frame_commit(self) -> FrameCommit {
        match self {
            Self::Committed => FrameCommit::Committed,
            Self::Rejected | Self::AcceptedPartial => FrameCommit::Unchanged,
        }
    }
}

/// Accept a monotonic projection and update the render snapshot once the
/// required domains supply current facts. Optional facets remain typed gaps.
/// The monotonicity policy and the
/// typed latest-state store live in `taskmanager-application`
/// ([`ProjectedSystemTelemetry::accept_projection`]); this wrapper only maps
/// the shared acceptance onto the renderer-neutral render model
/// (`Option<SystemSnapshot>`).
///
/// The result distinguishes a rejected projection, an accepted partial
/// projection, and a committed render snapshot. Frontends may ingest every
/// accepted outcome into history, but they must only replace their visible
/// frame on [`SystemTelemetryApply::Committed`].
pub(super) fn apply_projected_system_telemetry(
    latest: &mut Option<ProjectedSystemTelemetry>,
    snapshot: &mut Option<SystemSnapshot>,
    incoming: ProjectedSystemTelemetry,
) -> SystemTelemetryApply {
    let acceptance = ProjectedSystemTelemetry::accept_projection(latest, incoming);
    let result = SystemTelemetryApply::from_acceptance(&acceptance);
    if let ProjectionAcceptance::Accepted {
        snapshot: Some(incoming),
    } = acceptance
    {
        *snapshot = Some(*incoming);
    }
    result
}

impl SystemTelemetryApply {
    fn from_acceptance(acceptance: &ProjectionAcceptance) -> Self {
        match acceptance {
            ProjectionAcceptance::Rejected => Self::Rejected,
            ProjectionAcceptance::Accepted { snapshot: None } => Self::AcceptedPartial,
            ProjectionAcceptance::Accepted { snapshot: Some(_) } => Self::Committed,
        }
    }
}

/// Apply device lifecycle only from accepted observed outcomes. The shared
/// policy lives in `taskmanager-application::apply_system_outcome_lifecycle`.
pub(super) fn apply_system_outcome_lifecycle(
    projection: &mut DeviceLifecycleProjection,
    diagnostics: &mut DeviceLifecycleDiagnosticHistory,
    correlated: &CorrelatedSystemTelemetryOutcome,
) {
    taskmanager_application::apply_system_outcome_lifecycle(projection, diagnostics, correlated);
}

#[cfg(test)]
#[path = "../../tests/headless/shell_app_system_telemetry.rs"]
mod tests;
