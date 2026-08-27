//! Behavior tests for the scheduler-owned stall-abandonment deadline.

use taskmanager_application::{
    CapabilityId, ProviderId, RequestId, RequestScope, RequestTracking, SidebandPolicy,
};

use crate::config::{CapabilityRoute, DeliveryClass, RuntimeBudgets, RuntimeDomain};
use crate::health::CapabilityHealth;

use super::{
    CompletionRejection, CompletionVerdict, DEFAULT_IN_FLIGHT_LEASE_MS, EcsAdmissionError,
    RuntimeEcsScheduler,
};

fn request_id(value: u64) -> RequestId {
    RequestId::new(value).expect("fixture request id")
}

fn scope(value: u64) -> RequestScope {
    RequestScope::try_owned(format!("target:{value}")).expect("bounded fixture scope")
}

fn routes() -> Vec<CapabilityRoute> {
    vec![CapabilityRoute {
        capability: CapabilityId::DIRECTORY_USAGE,
        provider: ProviderId::borrowed("fixture.abandonment"),
        delivery: DeliveryClass::Observation,
        domain: RuntimeDomain::Storage,
        cadence_ms: None,
        sideband_policy: SidebandPolicy::Idempotent,
    }]
}

fn scheduler(lifetime_ms: u64) -> RuntimeEcsScheduler {
    RuntimeEcsScheduler::from_runtime_routes_with_budgets(
        &routes(),
        0,
        RuntimeBudgets {
            max_stalled_lifetime_ms: lifetime_ms,
            ..RuntimeBudgets::DEFAULT
        },
    )
}

#[test]
fn a_late_completion_inside_the_window_still_recovers_the_stall() {
    let mut scheduler = scheduler(5_000);
    let capability = CapabilityId::DIRECTORY_USAGE;
    let request = request_id(1);
    assert!(
        scheduler
            .admit_submission_with_tracking(&capability, request, 0, RequestTracking::Capability)
            .is_ok()
    );
    let stalled_at = DEFAULT_IN_FLIGHT_LEASE_MS;
    let plan = scheduler.tick_plan(stalled_at);
    assert_eq!(plan.stalled.len(), 1, "the lease must expire into a stall");
    assert_eq!(
        scheduler.scheduling_snapshot().active_stalled_capabilities,
        1
    );

    assert!(
        scheduler
            .claim_terminal_delivery(&capability, request)
            .is_accepted()
    );
    assert_eq!(
        scheduler.record_health_for_publication(
            &capability,
            request,
            CapabilityHealth::Available,
            stalled_at + 1,
        ),
        CompletionVerdict::Accepted(super::CompletionOwner::Capability),
        "an in-window late completion must recover the owner"
    );
    assert_eq!(scheduler.scheduling_snapshot().recovered_stalls, 1);
    assert_eq!(
        scheduler.scheduling_snapshot().active_stalled_capabilities,
        0
    );
}

#[test]
fn a_capability_stall_past_the_deadline_is_retired_and_the_route_requeues() {
    let lifetime = 5_000_u64;
    let mut scheduler = scheduler(lifetime);
    let capability = CapabilityId::DIRECTORY_USAGE;
    let zombie_request = request_id(2);
    assert!(
        scheduler
            .admit_submission_with_tracking(
                &capability,
                zombie_request,
                0,
                RequestTracking::Capability
            )
            .is_ok()
    );

    let stalled_at = DEFAULT_IN_FLIGHT_LEASE_MS;
    scheduler.tick_plan(stalled_at);
    assert_eq!(
        scheduler.scheduling_snapshot().budgets.pending_deliveries,
        1,
        "the stalled owner holds its delivery reservation inside the window"
    );
    assert!(
        scheduler
            .tick_plan(stalled_at + lifetime - 1)
            .stalled
            .is_empty(),
        "the stall is not re-reported while retained"
    );
    assert_eq!(
        scheduler.scheduling_snapshot().active_stalled_capabilities,
        1,
        "the owner is retained for a possible late completion"
    );

    scheduler.tick_plan(stalled_at + lifetime);
    let snapshot = scheduler.scheduling_snapshot();
    assert_eq!(snapshot.abandoned_stalls, 1);
    assert_eq!(snapshot.active_stalled_capabilities, 0);
    assert_eq!(
        snapshot.budgets.pending_deliveries, 0,
        "abandonment must recycle the delivery reservation"
    );

    assert_eq!(
        scheduler.record_health_for_publication(
            &capability,
            zombie_request,
            CapabilityHealth::Available,
            stalled_at + lifetime + 1,
        ),
        CompletionVerdict::Rejected(CompletionRejection::InactiveOwner),
        "the retired owner's very late completion must be rejected while the route is idle"
    );

    assert!(
        scheduler
            .admit_submission_with_tracking(
                &capability,
                request_id(3),
                stalled_at + lifetime + 2,
                RequestTracking::Capability
            )
            .is_ok(),
        "the route must accept a replacement submission after abandonment"
    );
    assert_eq!(
        scheduler.record_health_for_publication(
            &capability,
            zombie_request,
            CapabilityHealth::Available,
            stalled_at + lifetime + 3,
        ),
        CompletionVerdict::Rejected(CompletionRejection::RequestMismatch),
        "after replacement the zombie completion is rejected against the new owner"
    );
}

#[test]
fn a_target_stall_past_the_deadline_frees_the_scope_for_replacement() {
    let lifetime = 5_000_u64;
    let mut scheduler = scheduler(lifetime);
    let capability = CapabilityId::DIRECTORY_USAGE;
    let scope = scope(41);
    let zombie_request = request_id(4);
    assert!(
        scheduler
            .admit_submission_with_tracking(
                &capability,
                zombie_request,
                0,
                RequestTracking::Target(scope.clone()),
            )
            .is_ok()
    );

    let stalled_at = DEFAULT_IN_FLIGHT_LEASE_MS;
    scheduler.tick_plan(stalled_at);
    let snapshot = scheduler.scheduling_snapshot();
    assert_eq!(snapshot.active_target_jobs, 1);
    assert_eq!(snapshot.active_stalled_targets, 1);

    scheduler.tick_plan(stalled_at + lifetime);
    let snapshot = scheduler.scheduling_snapshot();
    assert_eq!(snapshot.target_abandoned_stalls, 1);
    assert_eq!(snapshot.active_target_jobs, 0);
    assert_eq!(snapshot.active_stalled_targets, 0);
    assert_eq!(
        snapshot.budgets.pending_deliveries, 0,
        "the abandoned target's delivery reservation must be recycled"
    );

    assert!(
        scheduler
            .admit_submission_with_tracking(
                &capability,
                request_id(5),
                stalled_at + lifetime,
                RequestTracking::Target(scope),
            )
            .is_ok(),
        "the abandoned scope must accept a replacement target request"
    );
    assert_eq!(
        scheduler.scheduling_snapshot().active_target_jobs,
        1,
        "the replacement target is tracked again"
    );
}

#[test]
fn an_in_flight_owner_is_never_abandoned() {
    let mut scheduler = scheduler(1);
    let capability = CapabilityId::DIRECTORY_USAGE;
    assert!(
        scheduler
            .admit_submission_with_tracking(
                &capability,
                request_id(6),
                0,
                RequestTracking::Capability
            )
            .is_ok()
    );
    // The lease has not expired yet: even a one-millisecond stall lifetime
    // must not touch the in-flight owner.
    let plan = scheduler.tick_plan(DEFAULT_IN_FLIGHT_LEASE_MS - 1);
    assert!(plan.stalled.is_empty());
    assert_eq!(scheduler.scheduling_snapshot().abandoned_stalls, 0);
    assert!(
        matches!(
            scheduler.admit_submission_with_tracking(
                &capability,
                request_id(7),
                DEFAULT_IN_FLIGHT_LEASE_MS - 1,
                RequestTracking::Capability
            ),
            Err(EcsAdmissionError::CapabilityInFlight)
        ),
        "the in-flight owner remains authoritative"
    );
}
