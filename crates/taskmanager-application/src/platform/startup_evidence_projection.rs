//! Application-owned startup evidence correlation and retention policy.

use taskmanager_core::{
    DeviceState, DeviceStatus, FailureKind, StartupBootEvidenceSnapshot, StartupEvidenceFailure,
};
use taskmanager_platform_contract::SubmissionErrorKind;

/// Application generation assigned when a startup-evidence refresh is submitted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StartupEvidenceRevision(u64);

impl StartupEvidenceRevision {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(next) => Some(Self(next)),
            None => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupEvidenceUnavailable {
    Provider(FailureKind),
    Submission(SubmissionErrorKind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedStartupEvidence {
    pub revision: StartupEvidenceRevision,
    pub snapshot: StartupBootEvidenceSnapshot,
    pub unavailable: Option<StartupEvidenceUnavailable>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupEvidenceProjectionRejection {
    NoActiveRequest,
    StaleOrUnexpectedRevision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartupEvidenceProjectionApplyResult {
    Applied(Box<ProjectedStartupEvidence>),
    Ignored(StartupEvidenceProjectionRejection),
}

/// Keeps concurrency, failure retention, and recovery semantics outside any
/// particular frontend.
#[derive(Clone, Debug, Default)]
pub struct StartupEvidenceProjection {
    active_revision: Option<StartupEvidenceRevision>,
    snapshot: Option<StartupBootEvidenceSnapshot>,
    unavailable: Option<StartupEvidenceUnavailable>,
}

impl StartupEvidenceProjection {
    pub fn begin(&mut self, revision: StartupEvidenceRevision) {
        if self
            .active_revision
            .is_none_or(|current| revision > current)
        {
            self.active_revision = Some(revision);
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> Option<ProjectedStartupEvidence> {
        Some(ProjectedStartupEvidence {
            revision: self.active_revision?,
            snapshot: self.snapshot.clone().unwrap_or_default(),
            unavailable: self.unavailable,
        })
    }

    pub fn apply(
        &mut self,
        revision: StartupEvidenceRevision,
        observation: StartupBootEvidenceSnapshot,
        observed_at_ms: u64,
    ) -> StartupEvidenceProjectionApplyResult {
        if let Err(rejection) = self.prepare(revision) {
            return StartupEvidenceProjectionApplyResult::Ignored(rejection);
        }
        self.snapshot = Some(merge_observation(
            self.snapshot.as_ref(),
            observation,
            observed_at_ms,
        ));
        self.unavailable = None;
        self.applied(revision)
    }

    pub fn apply_failure(
        &mut self,
        revision: StartupEvidenceRevision,
        unavailable: StartupEvidenceUnavailable,
        observed_at_ms: u64,
    ) -> StartupEvidenceProjectionApplyResult {
        if let Err(rejection) = self.prepare(revision) {
            return StartupEvidenceProjectionApplyResult::Ignored(rejection);
        }
        let failure = match unavailable {
            StartupEvidenceUnavailable::Provider(failure) => failure,
            StartupEvidenceUnavailable::Submission(kind) => submission_failure(kind),
        };
        self.snapshot = Some(mark_unavailable(
            self.snapshot.take().unwrap_or_default(),
            failure,
            observed_at_ms,
        ));
        self.unavailable = Some(unavailable);
        self.applied(revision)
    }

    fn prepare(
        &self,
        revision: StartupEvidenceRevision,
    ) -> Result<(), StartupEvidenceProjectionRejection> {
        let Some(active) = self.active_revision else {
            return Err(StartupEvidenceProjectionRejection::NoActiveRequest);
        };
        if revision != active {
            return Err(StartupEvidenceProjectionRejection::StaleOrUnexpectedRevision);
        }
        Ok(())
    }

    fn applied(&self, revision: StartupEvidenceRevision) -> StartupEvidenceProjectionApplyResult {
        StartupEvidenceProjectionApplyResult::Applied(Box::new(ProjectedStartupEvidence {
            revision,
            snapshot: self.snapshot.clone().unwrap_or_default(),
            unavailable: self.unavailable,
        }))
    }
}

fn merge_observation(
    previous: Option<&StartupBootEvidenceSnapshot>,
    mut incoming: StartupBootEvidenceSnapshot,
    observed_at_ms: u64,
) -> StartupBootEvidenceSnapshot {
    let Some(previous) = previous else {
        return incoming;
    };
    incoming.state = previous
        .state
        .merge_observation(incoming.state, observed_at_ms);
    incoming.failed_units_state = previous
        .failed_units_state
        .merge_observation(incoming.failed_units_state, observed_at_ms);
    incoming.critical_chain_state = previous
        .critical_chain_state
        .merge_observation(incoming.critical_chain_state, observed_at_ms);
    if incoming.failed_units_state.status != DeviceStatus::Healthy
        && incoming.failed_units.is_empty()
    {
        incoming.failed_units.clone_from(&previous.failed_units);
    }
    if incoming.critical_chain_state.status != DeviceStatus::Healthy
        && incoming.critical_chain.is_empty()
    {
        incoming.critical_chain.clone_from(&previous.critical_chain);
    }
    incoming
}

fn mark_unavailable(
    mut snapshot: StartupBootEvidenceSnapshot,
    failure: FailureKind,
    _observed_at_ms: u64,
) -> StartupBootEvidenceSnapshot {
    let status = DeviceStatus::from_failure(failure);
    snapshot.state = degraded(snapshot.state, status);
    snapshot.failed_units_state = degraded(snapshot.failed_units_state, status);
    snapshot.critical_chain_state = degraded(snapshot.critical_chain_state, status);
    let evidence_failure = evidence_failure(failure);
    snapshot.failed_units_failure = Some(evidence_failure);
    snapshot.critical_chain_failure = Some(evidence_failure);
    snapshot
}

const fn degraded(previous: DeviceState, status: DeviceStatus) -> DeviceState {
    DeviceState {
        status,
        last_success_ms: previous.last_success_ms,
    }
}

const fn evidence_failure(failure: FailureKind) -> StartupEvidenceFailure {
    match failure {
        FailureKind::MissingDependency => StartupEvidenceFailure::MissingTool,
        // RequiresEscalation is an escalatable denial; the startup-evidence
        // vocabulary has no escalation token, so fold it into PermissionDenied.
        FailureKind::PermissionDenied | FailureKind::RequiresEscalation => {
            StartupEvidenceFailure::PermissionDenied
        }
        FailureKind::TimedOut => StartupEvidenceFailure::TimedOut,
        FailureKind::Unsupported => StartupEvidenceFailure::Unsupported,
        FailureKind::IdentityChanged
        | FailureKind::TemporarilyUnavailable
        | FailureKind::Rejected
        | FailureKind::ProviderFault => StartupEvidenceFailure::Unavailable,
    }
}

const fn submission_failure(kind: SubmissionErrorKind) -> FailureKind {
    match kind {
        SubmissionErrorKind::Busy | SubmissionErrorKind::RuntimeStopped => {
            FailureKind::TemporarilyUnavailable
        }
        SubmissionErrorKind::UnsupportedCapability => FailureKind::Unsupported,
        SubmissionErrorKind::InvalidRequest => FailureKind::Rejected,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/platform/startup_evidence_projection.rs"]
mod tests;
