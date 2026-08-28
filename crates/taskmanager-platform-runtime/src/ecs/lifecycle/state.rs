//! Work-state transition rules for the ECS lifecycle.

use super::*;

impl WorkState {
    pub(super) fn begin_submission(
        &mut self,
        request_id: RequestId,
        submitted_at_monotonic_ms: u64,
    ) -> Result<(), EcsAdmissionError> {
        match self {
            Self::Waiting | Self::Ready => {
                *self = Self::InFlight {
                    request_id,
                    deadline_ms: submitted_at_monotonic_ms
                        .saturating_add(DEFAULT_IN_FLIGHT_LEASE_MS),
                };
                Ok(())
            }
            Self::InFlight { .. } => Err(EcsAdmissionError::CapabilityInFlight),
            Self::Stalled { .. } => Err(EcsAdmissionError::CapabilityStalled),
            Self::Blocked(_) => Err(EcsAdmissionError::CapabilityBlocked),
        }
    }

    pub(crate) fn expire_lease(
        &mut self,
        monotonic_now_ms: u64,
        stalled_lifetime_ms: u64,
    ) -> Option<RequestId> {
        let Self::InFlight {
            request_id,
            deadline_ms,
        } = *self
        else {
            return None;
        };
        if deadline_ms > monotonic_now_ms {
            return None;
        }
        *self = Self::Stalled {
            request_id,
            abandon_at_ms: monotonic_now_ms.saturating_add(stalled_lifetime_ms),
        };
        Some(request_id)
    }

    pub(crate) fn validate_owner(
        self,
        request_id: RequestId,
    ) -> Result<OwnedWorkPhase, CompletionRejection> {
        let (active_request, phase) = match self {
            Self::InFlight { request_id, .. } => (request_id, OwnedWorkPhase::InFlight),
            Self::Stalled { request_id, .. } => (request_id, OwnedWorkPhase::Stalled),
            Self::Waiting | Self::Ready | Self::Blocked(_) => {
                return Err(CompletionRejection::InactiveOwner);
            }
        };
        if active_request == request_id {
            Ok(phase)
        } else {
            Err(CompletionRejection::RequestMismatch)
        }
    }

    pub(super) fn finish(
        &mut self,
        request_id: RequestId,
        next: WorkState,
    ) -> Result<OwnedWorkPhase, CompletionRejection> {
        let phase = (*self).validate_owner(request_id)?;
        *self = next;
        Ok(phase)
    }

    pub(crate) fn renew_lease(
        &mut self,
        request_id: RequestId,
        renewed_at_monotonic_ms: u64,
    ) -> Result<OwnedWorkPhase, CompletionRejection> {
        let phase = (*self).validate_owner(request_id)?;
        *self = Self::InFlight {
            request_id,
            deadline_ms: renewed_at_monotonic_ms.saturating_add(DEFAULT_IN_FLIGHT_LEASE_MS),
        };
        Ok(phase)
    }

    pub(super) fn request_recovery(
        &mut self,
        trigger: CapabilityRecoveryTrigger,
    ) -> CapabilityRecoveryOutcome {
        match (*self, trigger) {
            (Self::InFlight { .. } | Self::Stalled { .. }, _) => {
                CapabilityRecoveryOutcome::ActiveOwner
            }
            (Self::Blocked(BlockedReason::Permanent), _) => {
                CapabilityRecoveryOutcome::PermanentlyBlocked
            }
            (
                Self::Blocked(BlockedReason::AwaitingCapabilityChange),
                CapabilityRecoveryTrigger::ExplicitRetry,
            ) => CapabilityRecoveryOutcome::AwaitingCapabilityChange,
            (
                Self::Waiting
                | Self::Ready
                | Self::Blocked(BlockedReason::AwaitingCapabilityChange),
                CapabilityRecoveryTrigger::CapabilityChanged,
            )
            | (Self::Waiting | Self::Ready, CapabilityRecoveryTrigger::ExplicitRetry) => {
                *self = Self::Waiting;
                CapabilityRecoveryOutcome::Ready
            }
        }
    }
}
