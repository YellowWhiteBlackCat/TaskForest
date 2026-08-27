//! Correlation and fail-closed publication for startup evidence.

use taskmanager_platform_contract::{CapabilityId, RequestId};

use crate::platform::{
    PlatformEventBatch, StartupEvidenceEvent, StartupEvidenceProjectionApplyResult,
    StartupEvidenceProjectionRejection, StartupEvidenceUnavailable,
};

use super::PlatformClient;

pub(super) struct StartupEvidenceDrainOutcome {
    pub(super) projection: Option<StartupEvidenceProjectionApplyResult>,
    pub(super) diagnostic: Option<taskmanager_core::FailureKind>,
}

impl StartupEvidenceDrainOutcome {
    fn discarded(failure: taskmanager_core::FailureKind) -> Self {
        Self {
            projection: None,
            diagnostic: Some(failure),
        }
    }
}

impl PlatformClient {
    pub(super) fn apply_startup_evidence_event(
        &mut self,
        request_id: RequestId,
        capability: &CapabilityId,
        event: &StartupEvidenceEvent,
        observed_at_ms: u64,
    ) -> StartupEvidenceDrainOutcome {
        let Some(revision) = self.startup_evidence_requests.remove(&request_id) else {
            return StartupEvidenceDrainOutcome::discarded(taskmanager_core::FailureKind::Rejected);
        };
        if capability != &CapabilityId::STARTUP_EVIDENCE || !event.accepts_capability(capability) {
            let failure = taskmanager_core::FailureKind::ProviderFault;
            return StartupEvidenceDrainOutcome {
                projection: Some(self.startup_evidence_projection.apply_failure(
                    revision,
                    StartupEvidenceUnavailable::Provider(failure),
                    observed_at_ms,
                )),
                diagnostic: Some(failure),
            };
        }
        let StartupEvidenceEvent::Snapshot(snapshot) = event;
        let applied =
            self.startup_evidence_projection
                .apply(revision, snapshot.clone(), observed_at_ms);
        let StartupEvidenceProjectionApplyResult::Ignored(rejection) = applied else {
            return StartupEvidenceDrainOutcome {
                projection: Some(applied),
                diagnostic: None,
            };
        };
        let failure = match rejection {
            StartupEvidenceProjectionRejection::NoActiveRequest => {
                taskmanager_core::FailureKind::Rejected
            }
            StartupEvidenceProjectionRejection::StaleOrUnexpectedRevision => {
                taskmanager_core::FailureKind::IdentityChanged
            }
        };
        StartupEvidenceDrainOutcome {
            projection: Some(self.startup_evidence_projection.apply_failure(
                revision,
                StartupEvidenceUnavailable::Provider(failure),
                observed_at_ms,
            )),
            diagnostic: Some(failure),
        }
    }
}

pub(super) fn append_startup_evidence_projection(
    batch: &mut PlatformEventBatch,
    result: Option<StartupEvidenceProjectionApplyResult>,
) {
    if let Some(StartupEvidenceProjectionApplyResult::Applied(projection)) = result {
        batch.startup_evidence_projections.push(*projection);
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/platform/client/startup_projection.rs"]
mod tests;
