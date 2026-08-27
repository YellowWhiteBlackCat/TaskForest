//! Correlation and fail-closed publication for system-domain completions.

use taskmanager_platform_contract::{CapabilityId, RequestId};

use super::PlatformClient;
use crate::platform::{
    PlatformEventBatch, SystemTelemetryDomain, SystemTelemetryDomainEvent,
    SystemTelemetryDomainOutcome, SystemTelemetryProjectionApplyResult,
    SystemTelemetryProjectionRejection, SystemTelemetryUnavailable,
};

pub(super) struct SystemEventProjectionOutcome {
    pub(super) projection: Option<SystemTelemetryProjectionApplyResult>,
    pub(super) rejection: Option<taskmanager_core::FailureKind>,
    pub(super) outcome: Option<SystemTelemetryDomainOutcome>,
}

impl SystemEventProjectionOutcome {
    fn accepted(
        event: &SystemTelemetryDomainEvent,
        projection: SystemTelemetryProjectionApplyResult,
    ) -> Self {
        Self {
            projection: Some(projection),
            rejection: None,
            outcome: Some(SystemTelemetryDomainOutcome::Observed(event.clone())),
        }
    }

    fn rejected_correlated(
        revision: crate::SystemTelemetryRevision,
        domain: SystemTelemetryDomain,
        projection: Option<SystemTelemetryProjectionApplyResult>,
        failure: taskmanager_core::FailureKind,
    ) -> Self {
        Self {
            projection,
            rejection: Some(failure),
            outcome: Some(SystemTelemetryDomainOutcome::Unavailable {
                revision,
                domain,
                reason: SystemTelemetryUnavailable::Provider(failure),
            }),
        }
    }

    fn discarded(failure: taskmanager_core::FailureKind) -> Self {
        Self {
            projection: None,
            rejection: Some(failure),
            outcome: None,
        }
    }
}

impl PlatformClient {
    pub(super) fn apply_system_telemetry_event(
        &mut self,
        request_id: RequestId,
        capability: &CapabilityId,
        event: &SystemTelemetryDomainEvent,
    ) -> SystemEventProjectionOutcome {
        let Some(pending) = self.system_telemetry_requests.remove(&request_id) else {
            return SystemEventProjectionOutcome::discarded(
                taskmanager_core::FailureKind::IdentityChanged,
            );
        };
        if pending.domain != event.domain()
            || system_telemetry_domain(capability) != Some(pending.domain)
        {
            let failure = taskmanager_core::FailureKind::ProviderFault;
            let projection = self.system_telemetry_projection.apply_failure(
                pending.revision,
                pending.domain,
                SystemTelemetryUnavailable::Provider(failure),
            );
            return SystemEventProjectionOutcome::rejected_correlated(
                pending.revision,
                pending.domain,
                Some(projection),
                failure,
            );
        }
        let applied = self.system_telemetry_projection.apply(event);
        let SystemTelemetryProjectionApplyResult::Ignored(rejection) = applied else {
            return SystemEventProjectionOutcome::accepted(event, applied);
        };
        let failure = match rejection {
            SystemTelemetryProjectionRejection::ConflictingDeviceLifecycle
            | SystemTelemetryProjectionRejection::StaleOrUnexpectedRevision => {
                taskmanager_core::FailureKind::IdentityChanged
            }
            SystemTelemetryProjectionRejection::NoActiveRequest
            | SystemTelemetryProjectionRejection::DuplicateDomain => {
                taskmanager_core::FailureKind::ProviderFault
            }
        };
        let projection = self.system_telemetry_projection.apply_failure(
            pending.revision,
            pending.domain,
            SystemTelemetryUnavailable::Provider(failure),
        );
        SystemEventProjectionOutcome::rejected_correlated(
            pending.revision,
            pending.domain,
            Some(projection),
            failure,
        )
    }
}

pub(super) fn append_system_projection(
    batch: &mut PlatformEventBatch,
    result: Option<SystemTelemetryProjectionApplyResult>,
) {
    match result {
        Some(SystemTelemetryProjectionApplyResult::AppliedPartial(projection))
        | Some(SystemTelemetryProjectionApplyResult::AppliedTerminal { projection }) => {
            batch.system_telemetry_projections.push(*projection);
        }
        Some(SystemTelemetryProjectionApplyResult::Ignored(_)) | None => {}
    }
}

pub(super) fn system_telemetry_domain(capability: &CapabilityId) -> Option<SystemTelemetryDomain> {
    SystemTelemetryDomain::from_capability(capability)
}

#[cfg(test)]
#[path = "../../../tests/headless/platform/client/system_projection.rs"]
mod tests;
