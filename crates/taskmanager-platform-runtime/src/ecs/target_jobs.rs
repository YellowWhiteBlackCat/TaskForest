//! Bounded lifecycle for independently addressed runtime jobs.
//!
//! Target jobs are ephemeral ECS entities indexed by request identity and by
//! stable target scope. A target remains owned after a lease expires so a late
//! worker completion cannot overlap a replacement request. The hard per-route
//! ceiling prevents a faulty worker that consumes without publishing from
//! growing the world indefinitely.

use std::collections::BTreeMap;

use bevy_ecs::component::Component;
use bevy_ecs::prelude::{Entity, Query, Res, ResMut};
use taskmanager_platform_contract::{CapabilityId, RequestId, RequestScope};

use super::{
    CompletionRejection, DEFAULT_IN_FLIGHT_LEASE_MS, EcsAdmissionError, EcsDiagnostics,
    OwnedWorkPhase, RuntimeEcsScheduler, SchedulerClock, StallPolicy, StalledSubject, StalledWork,
    WorkState,
};

#[derive(Component)]
pub(super) struct TargetJobNode {
    capability: CapabilityId,
    request_id: RequestId,
    scope: RequestScope,
    domain: crate::config::RuntimeDomain,
    scope_bytes: usize,
}

#[derive(Default)]
pub(super) struct TargetJobRegistry {
    jobs: BTreeMap<(CapabilityId, RequestId), Entity>,
    scopes: BTreeMap<(CapabilityId, RequestScope), RequestId>,
    active_by_domain: [usize; crate::config::RuntimeDomain::COUNT],
    scope_bytes: usize,
}

impl TargetJobRegistry {
    pub(super) fn len(&self) -> usize {
        self.jobs.len()
    }

    pub(super) fn contains_request(
        &self,
        capability: &CapabilityId,
        request_id: RequestId,
    ) -> bool {
        self.jobs.contains_key(&(capability.clone(), request_id))
    }

    pub(super) fn active_in_domain(&self, domain: crate::config::RuntimeDomain) -> usize {
        self.active_by_domain[domain.index()]
    }

    pub(super) fn scope_bytes(&self) -> usize {
        self.scope_bytes
    }
}

pub(super) fn mark_stalled_target_jobs_system(
    clock: Res<SchedulerClock>,
    stall_policy: Res<StallPolicy>,
    mut stalled_work: ResMut<StalledWork>,
    mut diagnostics: ResMut<EcsDiagnostics>,
    mut jobs: Query<(&TargetJobNode, &mut WorkState)>,
) {
    for (job, mut state) in &mut jobs {
        if let Some(request_id) =
            state.expire_lease(clock.monotonic_now_ms, stall_policy.lifetime_ms)
        {
            stalled_work.subjects.push(StalledSubject::Target {
                capability: job.capability.clone(),
                request_id,
                scope: job.scope.clone(),
            });
            diagnostics.stalled = diagnostics.stalled.saturating_add(1);
            diagnostics.target_stalled = diagnostics.target_stalled.saturating_add(1);
        }
    }
}

/// Retire target-job stalled owners whose abandonment deadline passed. The
/// entities are despawned by the scheduler's post-pass (the system cannot
/// reach the registry), which also recycles the delivery reservation and the
/// target scope for replacement requests. The state is deliberately left
/// untouched: if the despawn cannot run this tick the same owner is re-decided
/// next tick instead of being stranded in a mutated state.
pub(super) fn abandon_stalled_target_jobs_system(
    clock: Res<SchedulerClock>,
    mut abandoned: ResMut<super::AbandonedWork>,
    mut diagnostics: ResMut<EcsDiagnostics>,
    jobs: Query<(&TargetJobNode, &WorkState)>,
) {
    for (job, state) in &jobs {
        let WorkState::Stalled {
            request_id,
            abandon_at_ms,
        } = *state
        else {
            continue;
        };
        if abandon_at_ms > clock.monotonic_now_ms {
            continue;
        }
        abandoned.subjects.push(StalledSubject::Target {
            capability: job.capability.clone(),
            request_id,
            scope: job.scope.clone(),
        });
        diagnostics.target_abandoned_stalls = diagnostics.target_abandoned_stalls.saturating_add(1);
    }
}

impl RuntimeEcsScheduler {
    pub(super) fn validate_target_completion(
        &self,
        capability: &CapabilityId,
        request_id: RequestId,
    ) -> Result<OwnedWorkPhase, CompletionRejection> {
        let Some(entity) = self
            .target_jobs
            .jobs
            .get(&(capability.clone(), request_id))
            .copied()
        else {
            return Err(CompletionRejection::InactiveOwner);
        };
        self.world()
            .get_entity(entity)
            .ok()
            .ok_or(CompletionRejection::InvariantViolation)
            .and_then(|entity_ref| {
                let job = entity_ref
                    .get::<TargetJobNode>()
                    .ok_or(CompletionRejection::InvariantViolation)?;
                if job.capability != *capability || job.request_id != request_id {
                    return Err(CompletionRejection::InvariantViolation);
                }
                entity_ref
                    .get::<WorkState>()
                    .ok_or(CompletionRejection::InvariantViolation)?
                    .validate_owner(request_id)
            })
    }

    pub(crate) fn renew_target_lease(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        renewed_at_monotonic_ms: u64,
    ) -> Result<OwnedWorkPhase, CompletionRejection> {
        let key = (capability.clone(), request_id);
        let Some(entity) = self.target_jobs.jobs.get(&key).copied() else {
            return Err(CompletionRejection::InactiveOwner);
        };
        self.validate_target_completion(capability, request_id)?;
        let owner_phase = {
            let Some(mut state) = self.world_mut().get_mut::<WorkState>(entity) else {
                return Err(CompletionRejection::InvariantViolation);
            };
            state.renew_lease(request_id, renewed_at_monotonic_ms)?
        };
        if owner_phase == OwnedWorkPhase::Stalled {
            let mut diagnostics = self.world_mut().resource_mut::<EcsDiagnostics>();
            diagnostics.recovered_stalls = diagnostics.recovered_stalls.saturating_add(1);
            diagnostics.target_recovered_stalls =
                diagnostics.target_recovered_stalls.saturating_add(1);
        }
        Ok(owner_phase)
    }

    pub(super) fn try_reserve_target_submission(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        submitted_at_monotonic_ms: u64,
        scope: RequestScope,
    ) -> Result<(), EcsAdmissionError> {
        if !self.entities.contains_key(capability) {
            return Err(EcsAdmissionError::UnknownCapability);
        }
        if self
            .target_jobs
            .jobs
            .contains_key(&(capability.clone(), request_id))
        {
            return Err(EcsAdmissionError::DuplicateRequest);
        }
        if self
            .target_jobs
            .scopes
            .contains_key(&(capability.clone(), scope.clone()))
        {
            return Err(EcsAdmissionError::TargetInFlight);
        }
        let active_for_capability = self
            .target_jobs
            .scopes
            .keys()
            .filter(|(active_capability, _)| active_capability == capability)
            .count();
        if active_for_capability >= self.budgets.active_target_limit_per_capability {
            return Err(EcsAdmissionError::TargetCapacity);
        }
        if self.target_jobs.len() >= self.budgets.active_target_limit {
            return Err(EcsAdmissionError::GlobalTargetCapacity);
        }
        let Some(route_entity) = self.entities.get(capability).copied() else {
            return Err(EcsAdmissionError::UnknownCapability);
        };
        let Some((domain, delivery)) = self
            .world()
            .get::<super::CapabilityNode>(route_entity)
            .map(|node| (node.domain, node.delivery))
        else {
            return Err(EcsAdmissionError::InvariantViolation);
        };
        if self.target_jobs.active_in_domain(domain) >= self.budgets.active_target_limit_per_domain
        {
            return Err(EcsAdmissionError::DomainTargetCapacity);
        }
        let scope_bytes = scope.as_str().len();
        if self.target_jobs.scope_bytes().saturating_add(scope_bytes)
            > self.budgets.target_scope_byte_limit
        {
            return Err(EcsAdmissionError::TargetScopeByteCapacity);
        }
        self.reserve_delivery(capability, request_id, delivery)?;

        let entity = self
            .world_mut()
            .spawn((
                TargetJobNode {
                    capability: capability.clone(),
                    request_id,
                    scope: scope.clone(),
                    domain,
                    scope_bytes,
                },
                WorkState::InFlight {
                    request_id,
                    deadline_ms: submitted_at_monotonic_ms
                        .saturating_add(DEFAULT_IN_FLIGHT_LEASE_MS),
                },
            ))
            .id();
        self.target_jobs
            .jobs
            .insert((capability.clone(), request_id), entity);
        self.target_jobs
            .scopes
            .insert((capability.clone(), scope), request_id);
        self.target_jobs.active_by_domain[domain.index()] =
            self.target_jobs.active_by_domain[domain.index()].saturating_add(1);
        self.target_jobs.scope_bytes = self.target_jobs.scope_bytes.saturating_add(scope_bytes);
        let active_target_jobs = self.target_jobs.len() as u64;
        let mut diagnostics = self.world_mut().resource_mut::<EcsDiagnostics>();
        diagnostics.submissions = diagnostics.submissions.saturating_add(1);
        diagnostics.target_submissions = diagnostics.target_submissions.saturating_add(1);
        diagnostics.target_high_water = diagnostics.target_high_water.max(active_target_jobs);
        Ok(())
    }

    pub(super) fn complete_target_job(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
    ) -> Result<OwnedWorkPhase, CompletionRejection> {
        let owner_phase = self.remove_target_job(capability, request_id)?;
        let mut diagnostics = self.world_mut().resource_mut::<EcsDiagnostics>();
        diagnostics.completions = diagnostics.completions.saturating_add(1);
        diagnostics.target_completions = diagnostics.target_completions.saturating_add(1);
        diagnostics.recovered_stalls = diagnostics
            .recovered_stalls
            .saturating_add(owner_phase.recovered_stall_count());
        diagnostics.target_recovered_stalls = diagnostics
            .target_recovered_stalls
            .saturating_add(owner_phase.recovered_stall_count());
        Ok(owner_phase)
    }

    pub(super) fn remove_target_job(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
    ) -> Result<OwnedWorkPhase, CompletionRejection> {
        let key = (capability.clone(), request_id);
        let entity = self
            .target_jobs
            .jobs
            .get(&key)
            .copied()
            .ok_or(CompletionRejection::InactiveOwner)?;
        let (job_capability, job_request_id, scope, domain, scope_bytes, state) = self
            .world()
            .get_entity(entity)
            .ok()
            .and_then(|entity_ref| {
                let job = entity_ref.get::<TargetJobNode>()?;
                let state = *entity_ref.get::<WorkState>()?;
                Some((
                    job.capability.clone(),
                    job.request_id,
                    job.scope.clone(),
                    job.domain,
                    job.scope_bytes,
                    state,
                ))
            })
            .ok_or(CompletionRejection::InvariantViolation)?;
        if job_capability != *capability || job_request_id != request_id {
            return Err(CompletionRejection::InvariantViolation);
        }
        let owner_phase = state.validate_owner(request_id)?;

        if !self.world_mut().despawn(entity) {
            return Err(CompletionRejection::InvariantViolation);
        }
        self.target_jobs.jobs.remove(&key);
        let scope_key = (capability.clone(), scope);
        if self.target_jobs.scopes.get(&scope_key) == Some(&request_id) {
            self.target_jobs.scopes.remove(&scope_key);
        }
        self.target_jobs.active_by_domain[domain.index()] =
            self.target_jobs.active_by_domain[domain.index()].saturating_sub(1);
        self.target_jobs.scope_bytes = self.target_jobs.scope_bytes.saturating_sub(scope_bytes);
        Ok(owner_phase)
    }

    /// Despawn one target job whose stall-abandonment deadline passed.
    ///
    /// Unlike [`Self::remove_target_job`] this validates only registry
    /// identity, not work-state ownership: the abandonment decision itself is
    /// the authority that retires the owner, and it was taken by this tick's
    /// `abandon_stalled_target_jobs_system` over the exact same world. A very
    /// late completion of the retired request then fails its delivery claim
    /// and is tolerated as a counted stale publication.
    pub(super) fn remove_abandoned_target_job(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
    ) {
        let key = (capability.clone(), request_id);
        let Some(entity) = self.target_jobs.jobs.get(&key).copied() else {
            return;
        };
        let Some((job_capability, scope, domain, scope_bytes)) =
            self.world().get_entity(entity).ok().and_then(|entity_ref| {
                let job = entity_ref.get::<TargetJobNode>()?;
                Some((
                    job.capability.clone(),
                    job.scope.clone(),
                    job.domain,
                    job.scope_bytes,
                ))
            })
        else {
            return;
        };
        if job_capability != *capability {
            return;
        }
        if !self.world_mut().despawn(entity) {
            return;
        }
        self.target_jobs.jobs.remove(&key);
        let scope_key = (capability.clone(), scope);
        if self.target_jobs.scopes.get(&scope_key) == Some(&request_id) {
            self.target_jobs.scopes.remove(&scope_key);
        }
        self.target_jobs.active_by_domain[domain.index()] =
            self.target_jobs.active_by_domain[domain.index()].saturating_sub(1);
        self.target_jobs.scope_bytes = self.target_jobs.scope_bytes.saturating_sub(scope_bytes);
    }
}
