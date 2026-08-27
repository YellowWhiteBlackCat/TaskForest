use taskmanager_application::{
    CapabilityRecoveryOutcome, CapabilityRecoveryTrigger, ProviderFailure, RequestTracking,
    SchedulingDomain, SidebandPolicy,
};

use crate::config::{DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY, DeliveryClass, RuntimeBudgets};
use crate::health::CapabilityHealth;

use super::{
    CapabilityId, CapabilityRoute, CompletionOwner, CompletionRejection, CompletionVerdict,
    DEFAULT_IN_FLIGHT_LEASE_MS, EcsAdmissionError, EcsDiagnostics, ProviderId, RequestId,
    RequestScope, RuntimeEcsScheduler, StalledSubject, benchmark, replay,
};

#[path = "ecs_scheduler/target_jobs.rs"]
mod target_jobs;

impl EcsDiagnostics {
    pub(crate) const fn submission_count(self) -> u64 {
        self.submissions
    }

    pub(crate) const fn route_entity_count(self) -> u64 {
        self.route_entities
    }

    pub(crate) const fn duplicate_route_count(self) -> u64 {
        self.duplicate_routes
    }

    pub(crate) const fn stalled_count(self) -> u64 {
        self.stalled
    }

    pub(crate) const fn target_stalled_count(self) -> u64 {
        self.target_stalled
    }

    pub(crate) const fn target_high_water(self) -> u64 {
        self.target_high_water
    }

    pub(crate) const fn admission_rejections(self, error: EcsAdmissionError) -> u64 {
        match error {
            EcsAdmissionError::UnknownCapability => self.admission_unknown_capability,
            EcsAdmissionError::CapabilityInFlight => self.admission_capability_in_flight,
            EcsAdmissionError::CapabilityStalled => self.admission_capability_stalled,
            EcsAdmissionError::CapabilityBlocked => self.admission_capability_blocked,
            EcsAdmissionError::DuplicateRequest => self.admission_duplicate_request,
            EcsAdmissionError::TargetInFlight => self.admission_target_in_flight,
            EcsAdmissionError::TargetCapacity => self.admission_target_capacity,
            EcsAdmissionError::GlobalTargetCapacity => self.admission_global_target_capacity,
            EcsAdmissionError::DomainTargetCapacity => self.admission_domain_target_capacity,
            EcsAdmissionError::TargetScopeByteCapacity => self.admission_target_scope_byte_capacity,
            EcsAdmissionError::ControlDeliveryCapacity => self.admission_control_delivery_capacity,
            EcsAdmissionError::ObservationDeliveryCapacity => {
                self.admission_observation_delivery_capacity
            }
            EcsAdmissionError::SidebandNotAllowed => self.admission_sideband_not_allowed,
            EcsAdmissionError::InvariantViolation => self.admission_invariant_violation,
        }
    }
}

impl RuntimeEcsScheduler {
    pub(crate) fn record_health(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        health: CapabilityHealth,
        monotonic_now_ms: u64,
    ) -> CompletionVerdict {
        let claim = self.claim_terminal_delivery(capability, request_id);
        if !claim.is_accepted() {
            return claim;
        }
        let verdict =
            self.record_health_for_publication(capability, request_id, health, monotonic_now_ms);
        if verdict.is_accepted() {
            let _ = self.release_delivery(capability, request_id);
        }
        verdict
    }

    pub(crate) fn reserve_submission(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        submitted_at_monotonic_ms: u64,
    ) -> bool {
        self.admit_submission_with_tracking(
            capability,
            request_id,
            submitted_at_monotonic_ms,
            RequestTracking::Capability,
        )
        .is_ok()
    }

    pub(crate) fn reserve_submission_with_tracking(
        &mut self,
        capability: &CapabilityId,
        request_id: RequestId,
        submitted_at_monotonic_ms: u64,
        tracking: RequestTracking,
    ) -> bool {
        self.admit_submission_with_tracking(
            capability,
            request_id,
            submitted_at_monotonic_ms,
            tracking,
        )
        .is_ok()
    }

    fn tick(&mut self, now_ms: u64) -> Vec<CapabilityId> {
        self.tick_plan(now_ms)
            .items
            .into_iter()
            .map(|work| work.capability)
            .collect()
    }

    fn replay(
        &mut self,
        steps: impl IntoIterator<Item = replay::ReplayStep>,
    ) -> replay::ReplayReceipt {
        replay::run(self, steps)
    }

    fn entity_metadata(&mut self, capability: &CapabilityId) -> Option<(ProviderId, bool)> {
        let entity = self.entities.get(capability).copied()?;
        let node = self.world().get::<super::CapabilityNode>(entity)?;
        Some((
            node.provider.clone(),
            matches!(node.delivery, DeliveryClass::Observation),
        ))
    }

    pub(crate) fn diagnostics(&self) -> EcsDiagnostics {
        *self.world().resource::<EcsDiagnostics>()
    }

    pub(crate) fn target_job_count(&self) -> usize {
        self.target_jobs.len()
    }
}

fn routes() -> Vec<CapabilityRoute> {
    let provider = ProviderId::borrowed("fixture.ecs");
    vec![
        CapabilityRoute {
            capability: CapabilityId::TELEMETRY_CPU,
            provider: provider.clone(),
            delivery: DeliveryClass::Observation,
            domain: crate::config::RuntimeDomain::System,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        },
        CapabilityRoute {
            capability: CapabilityId::PROCESS_CONTROL,
            provider,
            delivery: DeliveryClass::Control,
            domain: crate::config::RuntimeDomain::Process,
            cadence_ms: Some(1_000),
            sideband_policy: SidebandPolicy::Denied,
        },
    ]
}

fn request_id(value: u64) -> RequestId {
    RequestId::new(value).expect("fixture request id")
}

fn target(value: impl Into<String>) -> RequestTracking {
    RequestTracking::Target(RequestScope::try_owned(value.into()).expect("bounded target fixture"))
}

#[test]
fn schedule_is_deterministic_and_does_not_duplicate_in_flight_work() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 100);
    assert_eq!(
        scheduler.tick(100),
        vec![CapabilityId::PROCESS_CONTROL, CapabilityId::TELEMETRY_CPU]
    );
    assert!(scheduler.reserve_submission(&CapabilityId::TELEMETRY_CPU, request_id(1), 100,));
    assert!(!scheduler.reserve_submission(&CapabilityId::TELEMETRY_CPU, request_id(2), 100,));
    assert!(scheduler.reserve_submission(&CapabilityId::PROCESS_CONTROL, request_id(3), 100,));
    assert!(scheduler.tick(101).is_empty());
}

#[test]
fn typed_work_plan_preserves_provider_and_delivery_attribution() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 100);
    let plan = scheduler.tick_plan(100);
    assert_eq!(plan.items.len(), 2);
    assert_eq!(plan.items[0].capability, CapabilityId::PROCESS_CONTROL);
    assert_eq!(plan.items[0].provider, ProviderId::borrowed("fixture.ecs"));
    assert_eq!(plan.items[0].delivery, DeliveryClass::Control);
    assert_eq!(plan.items[0].domain, crate::config::RuntimeDomain::Process);
    assert_eq!(plan.items[1].capability, CapabilityId::TELEMETRY_CPU);
    assert_eq!(plan.items[1].delivery, DeliveryClass::Observation);
    assert_eq!(plan.items[1].domain, crate::config::RuntimeDomain::System);
}

#[test]
fn duplicate_route_input_cannot_create_orphan_world_entities() {
    let mut duplicated = routes();
    duplicated.push(CapabilityRoute {
        capability: CapabilityId::TELEMETRY_CPU,
        provider: ProviderId::borrowed("fixture.duplicate"),
        delivery: DeliveryClass::Control,
        domain: crate::config::RuntimeDomain::Integration,
        cadence_ms: Some(1),
        sideband_policy: SidebandPolicy::Denied,
    });
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&duplicated, 0);
    let plan = scheduler.tick_plan(0);
    assert_eq!(
        plan.items.len(),
        2,
        "only unique capability routes may plan"
    );
    assert_eq!(
        plan.items
            .iter()
            .find(|item| item.capability == CapabilityId::TELEMETRY_CPU)
            .map(|item| (item.provider.clone(), item.delivery, item.domain)),
        Some((
            ProviderId::borrowed("fixture.ecs"),
            DeliveryClass::Observation,
            crate::config::RuntimeDomain::System,
        )),
        "the first typed route remains the deterministic authority"
    );
    let diagnostics = scheduler.diagnostics();
    assert_eq!(diagnostics.route_entity_count(), 2);
    assert_eq!(diagnostics.duplicate_route_count(), 1);
}

#[test]
fn domain_rollup_partitions_every_planned_item_without_growth_by_request() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 100);
    let plan = scheduler.tick_plan(100);
    let snapshot = scheduler.scheduling_snapshot();

    assert_eq!(snapshot.domains.len(), crate::config::RuntimeDomain::COUNT);
    assert_eq!(
        snapshot
            .domains
            .iter()
            .map(|domain| domain.planned_items)
            .sum::<u64>(),
        snapshot.planned_items
    );
    assert_eq!(snapshot.planned_items, plan.items.len() as u64);
    assert_eq!(
        snapshot
            .domains
            .iter()
            .find(|domain| domain.domain == SchedulingDomain::System)
            .map(|domain| domain.planned_items),
        Some(1)
    );
    assert_eq!(
        snapshot
            .domains
            .iter()
            .find(|domain| domain.domain == SchedulingDomain::Process)
            .map(|domain| domain.planned_items),
        Some(1)
    );
}

#[test]
fn completion_releases_the_capability_for_the_next_due_time() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    scheduler.tick(0);
    assert!(scheduler.reserve_submission(&CapabilityId::PROCESS_CONTROL, request_id(4), 0,));
    assert!(
        scheduler
            .record_health(
                &CapabilityId::PROCESS_CONTROL,
                request_id(4),
                CapabilityHealth::Available,
                100,
            )
            .is_accepted()
    );
    assert!(
        !scheduler
            .tick(1_099)
            .contains(&CapabilityId::PROCESS_CONTROL)
    );
    assert!(
        scheduler
            .tick(1_100)
            .contains(&CapabilityId::PROCESS_CONTROL)
    );
}

#[test]
fn failed_lane_submission_requeues_only_its_exact_owner() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    scheduler.tick(0);
    assert!(scheduler.reserve_submission(&CapabilityId::TELEMETRY_CPU, request_id(5), 0,));
    assert!(scheduler.cancel_submission(&CapabilityId::TELEMETRY_CPU, request_id(5), 500));
    assert!(!scheduler.tick(1_499).contains(&CapabilityId::TELEMETRY_CPU));
    assert!(scheduler.tick(1_500).contains(&CapabilityId::TELEMETRY_CPU));
}

#[test]
fn typed_health_bridge_respects_retry_disposition() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    assert!(scheduler.reserve_submission(&CapabilityId::TELEMETRY_CPU, request_id(6), 0,));
    assert!(
        scheduler
            .record_health(
                &CapabilityId::TELEMETRY_CPU,
                request_id(6),
                CapabilityHealth::Unavailable(ProviderFailure::TemporarilyUnavailable),
                0,
            )
            .is_accepted()
    );
    assert_eq!(
        scheduler.request_recovery(
            &CapabilityId::TELEMETRY_CPU,
            CapabilityRecoveryTrigger::ExplicitRetry,
            100,
        ),
        CapabilityRecoveryOutcome::Ready
    );
    assert!(scheduler.reserve_submission(&CapabilityId::TELEMETRY_CPU, request_id(106), 100,));
    assert!(
        scheduler
            .record_health(
                &CapabilityId::TELEMETRY_CPU,
                request_id(106),
                CapabilityHealth::Available,
                101,
            )
            .is_accepted()
    );
    assert!(!scheduler.tick(1_100).contains(&CapabilityId::TELEMETRY_CPU));
    assert!(scheduler.tick(1_101).contains(&CapabilityId::TELEMETRY_CPU));
}

#[test]
fn blocked_route_rejects_an_unowned_recovery_publication() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    assert!(scheduler.reserve_submission(&CapabilityId::TELEMETRY_CPU, request_id(7), 0,));
    assert!(
        scheduler
            .record_health(
                &CapabilityId::TELEMETRY_CPU,
                request_id(7),
                CapabilityHealth::Unavailable(ProviderFailure::Unsupported),
                0,
            )
            .is_accepted()
    );
    assert!(!scheduler.tick(1_000).contains(&CapabilityId::TELEMETRY_CPU));
    assert_eq!(
        scheduler.record_health(
            &CapabilityId::TELEMETRY_CPU,
            request_id(8),
            CapabilityHealth::Available,
            2_000,
        ),
        CompletionVerdict::Rejected(CompletionRejection::InactiveOwner)
    );
    assert!(!scheduler.tick(3_000).contains(&CapabilityId::TELEMETRY_CPU));
    for trigger in [
        CapabilityRecoveryTrigger::ExplicitRetry,
        CapabilityRecoveryTrigger::CapabilityChanged,
    ] {
        assert_eq!(
            scheduler.request_recovery(&CapabilityId::TELEMETRY_CPU, trigger, 3_001),
            CapabilityRecoveryOutcome::PermanentlyBlocked
        );
    }
}

#[test]
fn capability_change_is_required_before_retrying_a_prerequisite_blocked_route() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    let first = request_id(12);
    let retry = request_id(13);
    assert!(scheduler.reserve_submission(&CapabilityId::TELEMETRY_CPU, first, 0));
    assert!(
        scheduler
            .record_health(
                &CapabilityId::TELEMETRY_CPU,
                first,
                CapabilityHealth::Unavailable(ProviderFailure::PermissionDenied),
                0,
            )
            .is_accepted()
    );
    assert!(
        !scheduler
            .tick(10_000)
            .contains(&CapabilityId::TELEMETRY_CPU)
    );

    assert!(
        !scheduler.reserve_submission(&CapabilityId::TELEMETRY_CPU, retry, 10_001),
        "a plain request cannot pretend that permissions changed"
    );
    assert_eq!(
        scheduler.request_recovery(
            &CapabilityId::TELEMETRY_CPU,
            CapabilityRecoveryTrigger::ExplicitRetry,
            10_001,
        ),
        CapabilityRecoveryOutcome::AwaitingCapabilityChange
    );
    assert_eq!(
        scheduler.request_recovery(
            &CapabilityId::TELEMETRY_CPU,
            CapabilityRecoveryTrigger::CapabilityChanged,
            10_002,
        ),
        CapabilityRecoveryOutcome::Ready
    );
    assert!(
        scheduler
            .tick(10_002)
            .contains(&CapabilityId::TELEMETRY_CPU),
        "the armed route must be observable through the normal due-plan bridge"
    );
    assert!(scheduler.reserve_submission(&CapabilityId::TELEMETRY_CPU, retry, 10_002));
    assert!(
        scheduler
            .record_health(
                &CapabilityId::TELEMETRY_CPU,
                retry,
                CapabilityHealth::Available,
                10_003,
            )
            .is_accepted()
    );
    assert!(
        !scheduler
            .tick(11_002)
            .contains(&CapabilityId::TELEMETRY_CPU)
    );
    assert!(
        scheduler
            .tick(11_003)
            .contains(&CapabilityId::TELEMETRY_CPU)
    );
}

#[test]
fn enabling_manual_cadence_wakes_the_route_from_the_scheduler_clock() {
    let manual_route = [CapabilityRoute {
        capability: CapabilityId::SESSIONS,
        provider: ProviderId::borrowed("fixture.sessions"),
        delivery: DeliveryClass::Observation,
        domain: crate::config::RuntimeDomain::Environment,
        cadence_ms: None,
        sideband_policy: SidebandPolicy::Denied,
    }];
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&manual_route, 100);
    assert!(scheduler.tick(100).is_empty());
    assert!(scheduler.set_cadence_ms(&CapabilityId::SESSIONS, Some(10_000), 100));
    assert_eq!(scheduler.tick(100), vec![CapabilityId::SESSIONS]);
}

#[test]
fn diagnostics_count_kernel_lifecycle_activity() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    scheduler.tick(0);
    assert!(scheduler.reserve_submission(&CapabilityId::PROCESS_CONTROL, request_id(9), 0,));
    assert!(
        scheduler
            .record_health(
                &CapabilityId::PROCESS_CONTROL,
                request_id(9),
                CapabilityHealth::Available,
                0,
            )
            .is_accepted()
    );
    let diagnostics = scheduler.diagnostics();
    assert_eq!(
        (
            diagnostics.ticks,
            diagnostics.planned_items,
            diagnostics.submissions,
            diagnostics.completions
        ),
        (1, 2, 1, 1)
    );
}

#[test]
fn request_submission_and_completion_keep_the_same_request_identity() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    let submitted = request_id(10);
    let stale = request_id(11);
    assert!(scheduler.reserve_submission(&CapabilityId::TELEMETRY_CPU, submitted, 5));
    assert_eq!(
        scheduler.record_health(
            &CapabilityId::TELEMETRY_CPU,
            stale,
            CapabilityHealth::Available,
            6,
        ),
        CompletionVerdict::Rejected(CompletionRejection::RequestMismatch)
    );
    assert_eq!(
        scheduler.record_health(
            &CapabilityId::TELEMETRY_CPU,
            submitted,
            CapabilityHealth::Available,
            7,
        ),
        CompletionVerdict::Accepted(CompletionOwner::Capability)
    );
}

#[test]
fn expired_in_flight_lease_is_reported_once_and_late_completion_recovers() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    scheduler.tick_plan(0);
    let request = request_id(30);
    assert!(scheduler.reserve_submission(&CapabilityId::TELEMETRY_CPU, request, 0));
    assert!(
        scheduler
            .tick_plan(DEFAULT_IN_FLIGHT_LEASE_MS - 1)
            .stalled
            .is_empty()
    );
    let expired = scheduler.tick_plan(DEFAULT_IN_FLIGHT_LEASE_MS);
    assert_eq!(
        expired.stalled,
        vec![StalledSubject::Capability {
            capability: CapabilityId::TELEMETRY_CPU,
            request_id: request,
        }]
    );
    assert_eq!(scheduler.diagnostics().stalled_count(), 1);
    assert_eq!(
        scheduler.scheduling_snapshot().active_stalled_capabilities,
        1
    );
    let stalled_snapshot = scheduler.scheduling_snapshot();
    let stalled_diagnostics = scheduler.diagnostics();
    for trigger in [
        CapabilityRecoveryTrigger::ExplicitRetry,
        CapabilityRecoveryTrigger::CapabilityChanged,
    ] {
        assert_eq!(
            scheduler.request_recovery(
                &CapabilityId::TELEMETRY_CPU,
                trigger,
                DEFAULT_IN_FLIGHT_LEASE_MS + 1,
            ),
            CapabilityRecoveryOutcome::ActiveOwner,
            "recovery triggers cannot quarantine or replace a stalled owner"
        );
        assert_eq!(
            scheduler.scheduling_snapshot(),
            stalled_snapshot,
            "a recovery request cannot mutate an active stalled owner"
        );
        assert_eq!(
            scheduler.diagnostics(),
            stalled_diagnostics,
            "rejecting recovery cannot emit lifecycle side effects"
        );
    }
    assert!(
        scheduler
            .tick_plan(DEFAULT_IN_FLIGHT_LEASE_MS + 1)
            .stalled
            .is_empty(),
        "the same stalled request must not emit an unbounded timeout stream"
    );
    assert!(
        !scheduler.reserve_submission(
            &CapabilityId::TELEMETRY_CPU,
            request_id(31),
            DEFAULT_IN_FLIGHT_LEASE_MS + 1,
        ),
        "a stalled worker remains authoritative until its terminal publication"
    );
    let before_mismatched_completion = scheduler.scheduling_snapshot();
    assert_eq!(
        scheduler.record_health(
            &CapabilityId::TELEMETRY_CPU,
            request_id(31),
            CapabilityHealth::Available,
            DEFAULT_IN_FLIGHT_LEASE_MS + 2,
        ),
        CompletionVerdict::Rejected(CompletionRejection::RequestMismatch)
    );
    assert_eq!(
        scheduler.scheduling_snapshot(),
        before_mismatched_completion,
        "a mismatched late completion cannot release the stalled owner or its permit"
    );
    assert!(
        scheduler
            .record_health(
                &CapabilityId::TELEMETRY_CPU,
                request,
                CapabilityHealth::Available,
                DEFAULT_IN_FLIGHT_LEASE_MS + 2,
            )
            .is_accepted()
    );
    let recovered = scheduler.scheduling_snapshot();
    assert_eq!(recovered.active_stalled_capabilities, 0);
    assert_eq!(recovered.recovered_stalls, 1);
    assert!(
        scheduler
            .tick(DEFAULT_IN_FLIGHT_LEASE_MS + 1_001)
            .is_empty()
    );
    assert!(
        scheduler
            .tick(DEFAULT_IN_FLIGHT_LEASE_MS + 1_002)
            .contains(&CapabilityId::TELEMETRY_CPU)
    );
}

#[test]
fn route_metadata_remains_owned_and_queryable_inside_the_runtime_kernel() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    assert_eq!(
        scheduler.entity_metadata(&CapabilityId::TELEMETRY_CPU),
        Some((ProviderId::borrowed("fixture.ecs"), true))
    );
}

#[test]
fn headless_replay_reproduces_request_identity_and_due_order() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    let receipt = scheduler.replay([
        replay::ReplayStep::Tick { now_ms: 0 },
        replay::ReplayStep::Submitted {
            capability: CapabilityId::PROCESS_CONTROL,
            request_id: request_id(20),
            submitted_at_ms: 0,
        },
        replay::ReplayStep::Submitted {
            capability: CapabilityId::TELEMETRY_CPU,
            request_id: request_id(21),
            submitted_at_ms: 0,
        },
        replay::ReplayStep::Health {
            capability: CapabilityId::PROCESS_CONTROL,
            request_id: request_id(20),
            health: CapabilityHealth::Available,
            observed_at_ms: 0,
        },
        replay::ReplayStep::Health {
            capability: CapabilityId::TELEMETRY_CPU,
            request_id: request_id(21),
            health: CapabilityHealth::Available,
            observed_at_ms: 0,
        },
        replay::ReplayStep::Tick { now_ms: 1_000 },
    ]);
    assert_eq!(receipt.plans.len(), 2);
    assert_eq!(receipt.plans[0].items.len(), 2);
    assert_eq!(receipt.plans[1].items.len(), 2);
    assert_eq!(receipt.accepted_submissions, 2);
    assert_eq!(receipt.accepted_health, 2);
}

#[test]
fn benchmark_fixture_is_a_repeatable_headless_workload() {
    let sample = benchmark::run_fixture(3);
    assert_eq!(sample.iterations, 3);
    assert_eq!(sample.planned_items, 6);
    assert_eq!(sample.accepted_submissions, 6);
    assert_eq!(sample.accepted_health, 6);
    assert!(sample.elapsed_ns > 0);
}
