//! Capability-level admission, completion, retry, and recovery transitions.

use taskmanager_application::{
    CapabilityId, CapabilityRecoveryOutcome, CapabilityRecoveryTrigger, RequestId, RequestTracking,
    RetryDisposition,
};

use crate::health::CapabilityHealth;

mod state;

use super::{
    BlockedReason, CapabilityNode, CompletionOwner, CompletionRejection, CompletionVerdict,
    DEFAULT_IN_FLIGHT_LEASE_MS, DueAt, EcsAdmissionError, EcsDiagnostics, OwnedWorkPhase,
    RuntimeEcsScheduler, SchedulerClock, SidebandPolicy, WorkState,
};

impl RuntimeEcsScheduler {
    pub(crate) fn record_health_for_publication(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        health: CapabilityHealth,
        monotonic_now_ms: u64,
    ) -> CompletionVerdict {
        self.record_health_inner(capability, request_id, health, monotonic_now_ms)
    }

    fn record_health_inner(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        health: CapabilityHealth,
        monotonic_now_ms: u64,
    ) -> CompletionVerdict {
        if self.target_jobs.contains_request(capability, request_id) {
            if !self.terminal_delivery_is_claimed(capability, request_id) {
                return CompletionVerdict::Rejected(CompletionRejection::InactiveOwner);
            }
            return match self.complete_target_job(capability, request_id) {
                Ok(_) => CompletionVerdict::Accepted(CompletionOwner::Target),
                Err(rejection) => CompletionVerdict::Rejected(rejection),
            };
        }
        let owner = match self.validate_capability_completion(capability, request_id) {
            Ok(owner) => owner,
            Err(rejection) => return CompletionVerdict::Rejected(rejection),
        };
        if !self.terminal_delivery_is_claimed(capability, request_id) {
            return CompletionVerdict::Rejected(CompletionRejection::InactiveOwner);
        }
        let applied = match health {
            CapabilityHealth::Available | CapabilityHealth::Degraded(_) => {
                self.mark_completed(capability, request_id, monotonic_now_ms)
            }
            CapabilityHealth::Unavailable(error) => match error.retry() {
                RetryDisposition::RetryNow => {
                    self.requeue_owned(capability, request_id, monotonic_now_ms)
                }
                RetryDisposition::RetryLater => self.requeue_owned(
                    capability,
                    request_id,
                    monotonic_now_ms.saturating_add(self.retry_interval_ms),
                ),
                RetryDisposition::Never => {
                    self.block(capability, request_id, BlockedReason::Permanent)
                }
                RetryDisposition::AfterCapabilityChange => self.block(
                    capability,
                    request_id,
                    BlockedReason::AwaitingCapabilityChange,
                ),
            },
        };
        if applied {
            CompletionVerdict::Accepted(owner)
        } else {
            CompletionVerdict::Rejected(CompletionRejection::InvariantViolation)
        }
    }

    fn try_reserve_capability_submission(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        submitted_at_monotonic_ms: u64,
    ) -> Result<(), EcsAdmissionError> {
        let Some(entity) = self.entities.get(capability).copied() else {
            return Err(EcsAdmissionError::UnknownCapability);
        };
        let Some(state) = self.world().get::<WorkState>(entity).copied() else {
            return Err(EcsAdmissionError::InvariantViolation);
        };
        if self.world().get::<DueAt>(entity).is_none() {
            return Err(EcsAdmissionError::InvariantViolation);
        }
        let mut next_state = state;
        next_state.begin_submission(request_id, submitted_at_monotonic_ms)?;
        let Some(delivery) = self
            .world()
            .get::<CapabilityNode>(entity)
            .map(|node| node.delivery)
        else {
            return Err(EcsAdmissionError::InvariantViolation);
        };
        self.reserve_delivery(capability, request_id, delivery)?;

        let Some(mut state) = self.world_mut().get_mut::<WorkState>(entity) else {
            self.release_delivery(capability, request_id);
            return Err(EcsAdmissionError::InvariantViolation);
        };
        *state = next_state;
        if let Some(mut due_at) = self.world_mut().get_mut::<DueAt>(entity) {
            due_at.0 = submitted_at_monotonic_ms;
        }
        let mut diagnostics = self.world_mut().resource_mut::<EcsDiagnostics>();
        diagnostics.submissions = diagnostics.submissions.saturating_add(1);
        Ok(())
    }

    /// Admit one payload according to its typed lifecycle contract.
    pub(crate) fn admit_submission_with_tracking(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        submitted_at_monotonic_ms: u64,
        tracking: RequestTracking,
    ) -> Result<(), EcsAdmissionError> {
        let result = match tracking {
            RequestTracking::Capability => self.try_reserve_capability_submission(
                capability,
                request_id,
                submitted_at_monotonic_ms,
            ),
            RequestTracking::Target(scope) => self.try_reserve_target_submission(
                capability,
                request_id,
                submitted_at_monotonic_ms,
                scope,
            ),
            RequestTracking::Sideband => {
                let Some(entity) = self.entities.get(capability).copied() else {
                    return self.reject_admission(EcsAdmissionError::UnknownCapability);
                };
                match self
                    .world()
                    .get::<CapabilityNode>(entity)
                    .map(|node| node.sideband_policy)
                {
                    Some(SidebandPolicy::Idempotent) => Ok(()),
                    Some(SidebandPolicy::Denied) | None => {
                        Err(EcsAdmissionError::SidebandNotAllowed)
                    }
                }
            }
        };
        if let Err(error) = result {
            self.record_admission_rejection(error);
        }
        result
    }

    fn reject_admission(&mut self, error: EcsAdmissionError) -> Result<(), EcsAdmissionError> {
        self.record_admission_rejection(error);
        Err(error)
    }

    fn record_admission_rejection(&mut self, error: EcsAdmissionError) {
        let mut diagnostics = self.world_mut().resource_mut::<EcsDiagnostics>();
        let counter = match error {
            EcsAdmissionError::UnknownCapability => &mut diagnostics.admission_unknown_capability,
            EcsAdmissionError::CapabilityInFlight => {
                &mut diagnostics.admission_capability_in_flight
            }
            EcsAdmissionError::CapabilityStalled => &mut diagnostics.admission_capability_stalled,
            EcsAdmissionError::CapabilityBlocked => &mut diagnostics.admission_capability_blocked,
            EcsAdmissionError::DuplicateRequest => &mut diagnostics.admission_duplicate_request,
            EcsAdmissionError::TargetInFlight => &mut diagnostics.admission_target_in_flight,
            EcsAdmissionError::TargetCapacity => &mut diagnostics.admission_target_capacity,
            EcsAdmissionError::GlobalTargetCapacity => {
                &mut diagnostics.admission_global_target_capacity
            }
            EcsAdmissionError::DomainTargetCapacity => {
                &mut diagnostics.admission_domain_target_capacity
            }
            EcsAdmissionError::TargetScopeByteCapacity => {
                &mut diagnostics.admission_target_scope_byte_capacity
            }
            EcsAdmissionError::ControlDeliveryCapacity => {
                &mut diagnostics.admission_control_delivery_capacity
            }
            EcsAdmissionError::ObservationDeliveryCapacity => {
                &mut diagnostics.admission_observation_delivery_capacity
            }
            EcsAdmissionError::SidebandNotAllowed => {
                &mut diagnostics.admission_sideband_not_allowed
            }
            EcsAdmissionError::InvariantViolation => &mut diagnostics.admission_invariant_violation,
        };
        *counter = counter.saturating_add(1);
    }

    /// Roll back only the unowned `Ready` intent returned by `poll_due`.
    ///
    /// An explicit request may claim the same capability after planning but
    /// before the application submits the scheduled request. That explicit
    /// owner must remain authoritative when the scheduled submission reports
    /// `Busy`, so this transition deliberately refuses every owned state.
    pub(crate) fn requeue_planned_submission(
        &mut self,
        capability: &CapabilityId,
        failed_at_monotonic_ms: u64,
    ) -> bool {
        let Some(entity) = self.entities.get(capability).copied() else {
            return false;
        };
        let ready_and_unowned = self
            .world()
            .get::<WorkState>(entity)
            .is_some_and(|state| *state == WorkState::Ready);
        if !ready_and_unowned {
            return false;
        }
        self.requeue_unowned(
            capability,
            failed_at_monotonic_ms.saturating_add(self.retry_interval_ms),
        )
    }

    /// Roll back a reservation rejected by the bounded worker lane.
    pub(crate) fn cancel_submission(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        failed_at_monotonic_ms: u64,
    ) -> bool {
        if self.remove_target_job(capability, request_id).is_ok() {
            self.release_delivery(capability, request_id);
            let mut diagnostics = self.world_mut().resource_mut::<EcsDiagnostics>();
            diagnostics.target_cancellations = diagnostics.target_cancellations.saturating_add(1);
            return true;
        }
        let Some(entity) = self.entities.get(capability).copied() else {
            return false;
        };
        let matches_request = self
            .world()
            .get::<WorkState>(entity)
            .is_some_and(|state| state.validate_owner(request_id).is_ok());
        if !matches_request {
            return false;
        }
        let requeued = self.requeue_owned(
            capability,
            request_id,
            failed_at_monotonic_ms.saturating_add(self.retry_interval_ms),
        );
        if requeued {
            self.release_delivery(capability, request_id);
        }
        requeued
    }

    pub(super) fn reserve_delivery(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        class: crate::config::DeliveryClass,
    ) -> Result<(), EcsAdmissionError> {
        let key = (capability.clone(), request_id);
        if self.delivery_reservations.contains_key(&key) {
            return Err(EcsAdmissionError::DuplicateRequest);
        }
        let rejection = match class {
            crate::config::DeliveryClass::Control => EcsAdmissionError::ControlDeliveryCapacity,
            crate::config::DeliveryClass::Observation => {
                EcsAdmissionError::ObservationDeliveryCapacity
            }
        };
        if self.delivery_reservations.len() >= self.budgets.pending_delivery_limit {
            return Err(rejection);
        }
        if class == crate::config::DeliveryClass::Observation
            && self.pending_deliveries(class)
                >= self
                    .budgets
                    .pending_delivery_limit
                    .saturating_sub(self.budgets.control_delivery_reserve)
        {
            return Err(rejection);
        }
        self.delivery_reservations.insert(
            key,
            super::DeliveryReservation {
                class,
                state: super::DeliveryReservationState::Active,
            },
        );
        Ok(())
    }

    pub(super) fn pending_deliveries(&self, class: crate::config::DeliveryClass) -> usize {
        self.delivery_reservations
            .values()
            .filter(|reservation| reservation.class == class)
            .count()
    }

    pub(super) fn release_delivery(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
    ) -> bool {
        self.delivery_reservations
            .remove(&(capability.clone(), request_id))
            .is_some()
    }

    fn terminal_delivery_is_claimed(
        &self,
        capability: &CapabilityId,
        request_id: RequestId,
    ) -> bool {
        self.delivery_reservations
            .get(&(capability.clone(), request_id))
            .is_some_and(|reservation| {
                reservation.state == super::DeliveryReservationState::TerminalClaimed
            })
    }

    pub(crate) fn claim_terminal_delivery(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
    ) -> CompletionVerdict {
        let owner = if self.target_jobs.contains_request(capability, request_id) {
            match self.validate_target_completion(capability, request_id) {
                Ok(_) => CompletionOwner::Target,
                Err(rejection) => return CompletionVerdict::Rejected(rejection),
            }
        } else {
            match self.validate_capability_completion(capability, request_id) {
                Ok(owner) => owner,
                Err(rejection) => return CompletionVerdict::Rejected(rejection),
            }
        };
        let Some(state) = self
            .delivery_reservations
            .get_mut(&(capability.clone(), request_id))
        else {
            return CompletionVerdict::Rejected(CompletionRejection::InactiveOwner);
        };
        if state.state != super::DeliveryReservationState::Active {
            return CompletionVerdict::Rejected(CompletionRejection::InactiveOwner);
        }
        state.state = super::DeliveryReservationState::TerminalClaimed;
        CompletionVerdict::Accepted(owner)
    }

    pub(crate) fn abort_terminal_delivery(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
    ) -> bool {
        let Some(state) = self
            .delivery_reservations
            .get_mut(&(capability.clone(), request_id))
        else {
            return false;
        };
        if state.state != super::DeliveryReservationState::TerminalClaimed {
            return false;
        }
        state.state = super::DeliveryReservationState::Active;
        true
    }

    pub(crate) fn acknowledge_terminal_delivery(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
    ) -> bool {
        if self
            .delivery_reservations
            .get(&(capability.clone(), request_id))
            .is_none_or(|reservation| {
                reservation.state != super::DeliveryReservationState::TerminalClaimed
            })
        {
            return false;
        }
        self.release_delivery(capability, request_id)
    }

    /// Change automatic cadence without changing the current lifecycle.
    pub(crate) fn set_cadence_ms(
        &mut self,
        capability: &CapabilityId,
        cadence_ms: Option<u64>,
        monotonic_now_ms: u64,
    ) -> bool {
        let Some(entity) = self.entities.get(capability).copied() else {
            return false;
        };
        self.world_mut()
            .resource_mut::<SchedulerClock>()
            .monotonic_now_ms = monotonic_now_ms;
        let waiting = self
            .world()
            .get::<WorkState>(entity)
            .is_some_and(|state| *state == WorkState::Waiting);
        let previous_cadence = self
            .world()
            .get::<CapabilityNode>(entity)
            .and_then(|node| node.cadence_ms);
        let previous_due_at = self.world().get::<DueAt>(entity).map(|due| due.0);
        {
            let Some(mut node) = self.world_mut().get_mut::<CapabilityNode>(entity) else {
                return false;
            };
            node.cadence_ms = cadence_ms;
        }
        if waiting && let Some(mut due_at) = self.world_mut().get_mut::<DueAt>(entity) {
            due_at.0 = match cadence_ms {
                None => u64::MAX,
                Some(_) if previous_cadence.is_none() => monotonic_now_ms,
                Some(_) => previous_due_at.unwrap_or(monotonic_now_ms),
            };
        }
        true
    }

    fn validate_capability_completion(
        &self,
        capability: &CapabilityId,
        request_id: RequestId,
    ) -> Result<CompletionOwner, CompletionRejection> {
        let Some(entity) = self.entities.get(capability).copied() else {
            return Err(CompletionRejection::UnknownCapability);
        };
        let Some(state) = self.world().get::<WorkState>(entity).copied() else {
            return Err(CompletionRejection::InvariantViolation);
        };
        state.validate_owner(request_id)?;
        Ok(CompletionOwner::Capability)
    }

    fn mark_completed(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        completed_at_monotonic_ms: u64,
    ) -> bool {
        let Some(entity) = self.entities.get(capability).copied() else {
            return false;
        };
        let owner_phase = {
            let Some(mut state) = self.world_mut().get_mut::<WorkState>(entity) else {
                return false;
            };
            let Ok(owner_phase) = state.finish(request_id, WorkState::Waiting) else {
                return false;
            };
            owner_phase
        };
        let cadence_ms = self
            .world()
            .get::<CapabilityNode>(entity)
            .and_then(|node| node.cadence_ms);
        if let Some(mut due_at) = self.world_mut().get_mut::<DueAt>(entity) {
            due_at.0 = cadence_ms
                .map(|cadence| completed_at_monotonic_ms.saturating_add(cadence))
                .unwrap_or(u64::MAX);
        }
        let mut diagnostics = self.world_mut().resource_mut::<EcsDiagnostics>();
        diagnostics.completions = diagnostics.completions.saturating_add(1);
        diagnostics.recovered_stalls = diagnostics
            .recovered_stalls
            .saturating_add(owner_phase.recovered_stall_count());
        true
    }

    fn requeue_unowned(&mut self, capability: &CapabilityId, next_due_monotonic_ms: u64) -> bool {
        let Some(entity) = self.entities.get(capability).copied() else {
            return false;
        };
        {
            let Some(mut state) = self.world_mut().get_mut::<WorkState>(entity) else {
                return false;
            };
            if *state != WorkState::Ready {
                return false;
            }
            *state = WorkState::Waiting;
        }
        if let Some(mut due_at) = self.world_mut().get_mut::<DueAt>(entity) {
            due_at.0 = next_due_monotonic_ms;
        }
        let mut diagnostics = self.world_mut().resource_mut::<EcsDiagnostics>();
        diagnostics.requeues = diagnostics.requeues.saturating_add(1);
        true
    }

    fn requeue_owned(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        next_due_monotonic_ms: u64,
    ) -> bool {
        let Some(entity) = self.entities.get(capability).copied() else {
            return false;
        };
        let owner_phase = {
            let Some(mut state) = self.world_mut().get_mut::<WorkState>(entity) else {
                return false;
            };
            let Ok(owner_phase) = state.finish(request_id, WorkState::Waiting) else {
                return false;
            };
            owner_phase
        };
        if let Some(mut due_at) = self.world_mut().get_mut::<DueAt>(entity) {
            due_at.0 = next_due_monotonic_ms;
        }
        let mut diagnostics = self.world_mut().resource_mut::<EcsDiagnostics>();
        diagnostics.requeues = diagnostics.requeues.saturating_add(1);
        diagnostics.recovered_stalls = diagnostics
            .recovered_stalls
            .saturating_add(owner_phase.recovered_stall_count());
        true
    }

    fn block(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        reason: BlockedReason,
    ) -> bool {
        let Some(entity) = self.entities.get(capability).copied() else {
            return false;
        };
        let owner_phase = {
            let Some(mut state) = self.world_mut().get_mut::<WorkState>(entity) else {
                return false;
            };
            let Ok(owner_phase) = state.finish(request_id, WorkState::Blocked(reason)) else {
                return false;
            };
            owner_phase
        };
        if let Some(mut due_at) = self.world_mut().get_mut::<DueAt>(entity) {
            due_at.0 = u64::MAX;
        }
        let mut diagnostics = self.world_mut().resource_mut::<EcsDiagnostics>();
        diagnostics.blocked = diagnostics.blocked.saturating_add(1);
        diagnostics.recovered_stalls = diagnostics
            .recovered_stalls
            .saturating_add(owner_phase.recovered_stall_count());
        true
    }

    pub(crate) fn request_recovery(
        &mut self,
        capability: &CapabilityId,
        trigger: CapabilityRecoveryTrigger,
        monotonic_now_ms: u64,
    ) -> CapabilityRecoveryOutcome {
        let Some(entity) = self.entities.get(capability).copied() else {
            return CapabilityRecoveryOutcome::UnknownCapability;
        };
        let outcome = {
            let Some(mut state) = self.world_mut().get_mut::<WorkState>(entity) else {
                return CapabilityRecoveryOutcome::UnknownCapability;
            };
            state.request_recovery(trigger)
        };
        if outcome == CapabilityRecoveryOutcome::Ready
            && let Some(mut due_at) = self.world_mut().get_mut::<DueAt>(entity)
        {
            due_at.0 = monotonic_now_ms;
        }
        outcome
    }
}
