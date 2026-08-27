//! test-intent: behavior

use super::*;

#[test]
fn target_jobs_admit_distinct_scopes_and_reclaim_terminal_entities() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    let first = request_id(40);
    let second = request_id(41);
    assert!(scheduler.reserve_submission_with_tracking(
        &CapabilityId::TELEMETRY_CPU,
        first,
        0,
        target("disk:a"),
    ));
    assert!(scheduler.reserve_submission_with_tracking(
        &CapabilityId::TELEMETRY_CPU,
        second,
        0,
        target("disk:b"),
    ));
    assert!(!scheduler.reserve_submission_with_tracking(
        &CapabilityId::TELEMETRY_CPU,
        request_id(42),
        0,
        target("disk:a"),
    ));
    assert_eq!(scheduler.target_job_count(), 2);

    assert!(
        scheduler
            .record_health(
                &CapabilityId::TELEMETRY_CPU,
                first,
                CapabilityHealth::Available,
                1,
            )
            .is_accepted()
    );
    assert_eq!(scheduler.target_job_count(), 1);
    assert!(scheduler.reserve_submission_with_tracking(
        &CapabilityId::TELEMETRY_CPU,
        request_id(42),
        2,
        target("disk:a"),
    ));
    assert!(
        scheduler
            .record_health(
                &CapabilityId::TELEMETRY_CPU,
                second,
                CapabilityHealth::Unavailable(ProviderFailure::TimedOut),
                3,
            )
            .is_accepted()
    );
    assert!(
        scheduler
            .record_health(
                &CapabilityId::TELEMETRY_CPU,
                request_id(42),
                CapabilityHealth::Available,
                3,
            )
            .is_accepted()
    );
    assert_eq!(scheduler.target_job_count(), 0);
}

#[test]
fn admission_rejections_keep_exact_contention_causes() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    assert!(scheduler.reserve_submission(&CapabilityId::TELEMETRY_CPU, request_id(60), 0));
    assert_eq!(
        scheduler.admit_submission_with_tracking(
            &CapabilityId::TELEMETRY_CPU,
            request_id(61),
            1,
            RequestTracking::Capability,
        ),
        Err(EcsAdmissionError::CapabilityInFlight)
    );
    scheduler.tick_plan(DEFAULT_IN_FLIGHT_LEASE_MS);
    assert_eq!(
        scheduler.admit_submission_with_tracking(
            &CapabilityId::TELEMETRY_CPU,
            request_id(62),
            DEFAULT_IN_FLIGHT_LEASE_MS,
            RequestTracking::Capability,
        ),
        Err(EcsAdmissionError::CapabilityStalled)
    );

    let mut targets = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    assert!(
        targets
            .admit_submission_with_tracking(
                &CapabilityId::PROCESS_CONTROL,
                request_id(70),
                0,
                target("process:1"),
            )
            .is_ok()
    );
    assert_eq!(
        targets.admit_submission_with_tracking(
            &CapabilityId::PROCESS_CONTROL,
            request_id(70),
            0,
            target("process:2"),
        ),
        Err(EcsAdmissionError::DuplicateRequest)
    );
    assert_eq!(
        targets.admit_submission_with_tracking(
            &CapabilityId::PROCESS_CONTROL,
            request_id(71),
            0,
            target("process:1"),
        ),
        Err(EcsAdmissionError::TargetInFlight)
    );
    assert_eq!(
        targets.admit_submission_with_tracking(
            &CapabilityId::borrowed("fixture.unknown"),
            request_id(72),
            0,
            RequestTracking::Sideband,
        ),
        Err(EcsAdmissionError::UnknownCapability)
    );
    assert_eq!(
        targets.admit_submission_with_tracking(
            &CapabilityId::PROCESS_CONTROL,
            request_id(73),
            0,
            RequestTracking::Sideband,
        ),
        Err(EcsAdmissionError::SidebandNotAllowed)
    );

    assert_eq!(
        scheduler
            .diagnostics()
            .admission_rejections(EcsAdmissionError::CapabilityInFlight),
        1
    );
    assert_eq!(
        scheduler
            .diagnostics()
            .admission_rejections(EcsAdmissionError::CapabilityStalled),
        1
    );
    for error in [
        EcsAdmissionError::DuplicateRequest,
        EcsAdmissionError::TargetInFlight,
        EcsAdmissionError::UnknownCapability,
        EcsAdmissionError::SidebandNotAllowed,
    ] {
        assert_eq!(targets.diagnostics().admission_rejections(error), 1);
    }
}

#[test]
fn target_job_cardinality_has_a_hard_per_capability_ceiling() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    for index in 0..DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY {
        assert!(scheduler.reserve_submission_with_tracking(
            &CapabilityId::TELEMETRY_CPU,
            request_id(index as u64 + 100),
            0,
            target(format!("target:{index}")),
        ));
    }
    assert_eq!(
        scheduler.admit_submission_with_tracking(
            &CapabilityId::TELEMETRY_CPU,
            request_id(999),
            0,
            target("one-too-many"),
        ),
        Err(EcsAdmissionError::TargetCapacity)
    );
    assert_eq!(
        scheduler
            .diagnostics()
            .admission_rejections(EcsAdmissionError::TargetCapacity),
        1
    );
    assert_eq!(
        scheduler.target_job_count(),
        DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY
    );
    assert_eq!(
        scheduler.diagnostics().target_high_water(),
        DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY as u64
    );

    for index in 0..DEFAULT_ACTIVE_TARGET_LIMIT_PER_CAPABILITY {
        assert!(
            scheduler
                .record_health(
                    &CapabilityId::TELEMETRY_CPU,
                    request_id(index as u64 + 100),
                    CapabilityHealth::Available,
                    1,
                )
                .is_accepted()
        );
    }
    assert_eq!(scheduler.target_job_count(), 0);
}

#[test]
fn global_domain_and_scope_byte_budgets_reject_without_partial_admission() {
    let global_budgets = RuntimeBudgets {
        route_limit: 2,
        active_target_limit: 2,
        active_target_limit_per_capability: 2,
        active_target_limit_per_domain: 2,
        target_scope_byte_limit: 64,
        pending_delivery_limit: 5,
        control_delivery_reserve: 1,
        max_stalled_lifetime_ms: RuntimeBudgets::DEFAULT.max_stalled_lifetime_ms,
    };
    let mut global =
        RuntimeEcsScheduler::from_runtime_routes_with_budgets(&routes(), 0, global_budgets);
    assert!(global.reserve_submission_with_tracking(
        &CapabilityId::TELEMETRY_CPU,
        request_id(1_001),
        0,
        target("system:a"),
    ));
    assert!(global.reserve_submission_with_tracking(
        &CapabilityId::PROCESS_CONTROL,
        request_id(1_002),
        0,
        target("process:a"),
    ));
    assert_eq!(
        global.admit_submission_with_tracking(
            &CapabilityId::TELEMETRY_CPU,
            request_id(1_003),
            0,
            target("system:b"),
        ),
        Err(EcsAdmissionError::GlobalTargetCapacity)
    );
    assert_eq!(global.target_job_count(), 2);

    let domain_budgets = RuntimeBudgets {
        active_target_limit: 4,
        active_target_limit_per_capability: 3,
        active_target_limit_per_domain: 2,
        pending_delivery_limit: 7,
        max_stalled_lifetime_ms: RuntimeBudgets::DEFAULT.max_stalled_lifetime_ms,
        ..global_budgets
    };
    let mut domain =
        RuntimeEcsScheduler::from_runtime_routes_with_budgets(&routes(), 0, domain_budgets);
    for (request, target_scope) in [(1_011, "domain:a"), (1_012, "domain:b")] {
        assert!(domain.reserve_submission_with_tracking(
            &CapabilityId::TELEMETRY_CPU,
            request_id(request),
            0,
            target(target_scope),
        ));
    }
    assert_eq!(
        domain.admit_submission_with_tracking(
            &CapabilityId::TELEMETRY_CPU,
            request_id(1_013),
            0,
            target("domain:c"),
        ),
        Err(EcsAdmissionError::DomainTargetCapacity)
    );
    assert_eq!(domain.target_job_count(), 2);

    let byte_budgets = RuntimeBudgets {
        active_target_limit: 4,
        active_target_limit_per_capability: 4,
        active_target_limit_per_domain: 4,
        target_scope_byte_limit: 10,
        pending_delivery_limit: 7,
        max_stalled_lifetime_ms: RuntimeBudgets::DEFAULT.max_stalled_lifetime_ms,
        ..global_budgets
    };
    let mut bytes =
        RuntimeEcsScheduler::from_runtime_routes_with_budgets(&routes(), 0, byte_budgets);
    assert!(bytes.reserve_submission_with_tracking(
        &CapabilityId::TELEMETRY_CPU,
        request_id(1_021),
        0,
        target("123456"),
    ));
    assert_eq!(
        bytes.admit_submission_with_tracking(
            &CapabilityId::TELEMETRY_CPU,
            request_id(1_022),
            0,
            target("abcdef"),
        ),
        Err(EcsAdmissionError::TargetScopeByteCapacity)
    );
    let snapshot = bytes.scheduling_snapshot();
    assert_eq!(snapshot.active_target_jobs, 1);
    assert_eq!(snapshot.budgets.active_target_scope_bytes, 6);
    assert_eq!(snapshot.admission.target_scope_byte_capacity, 1);
}

#[test]
fn stalled_target_job_reports_once_and_keeps_scope_until_late_completion() {
    let mut scheduler = RuntimeEcsScheduler::from_runtime_routes(&routes(), 0);
    let request = request_id(50);
    assert!(scheduler.reserve_submission_with_tracking(
        &CapabilityId::TELEMETRY_CPU,
        request,
        0,
        target("disk:slow"),
    ));
    let stalled = scheduler.tick_plan(DEFAULT_IN_FLIGHT_LEASE_MS);
    assert_eq!(
        stalled.stalled,
        vec![StalledSubject::Target {
            capability: CapabilityId::TELEMETRY_CPU,
            request_id: request,
            scope: RequestScope::try_from_str("disk:slow").expect("bounded target fixture"),
        }]
    );
    assert_eq!(scheduler.diagnostics().target_stalled_count(), 1);
    assert_eq!(scheduler.scheduling_snapshot().active_stalled_targets, 1);
    assert_eq!(
        scheduler.scheduling_snapshot().budgets.pending_deliveries,
        1,
        "a never-returning target retains exactly one owner and delivery permit"
    );
    assert!(
        scheduler
            .tick_plan(DEFAULT_IN_FLIGHT_LEASE_MS + 1)
            .stalled
            .is_empty()
    );
    let before_unknown_completion = scheduler.scheduling_snapshot();
    assert_eq!(
        scheduler.record_health(
            &CapabilityId::TELEMETRY_CPU,
            request_id(51),
            CapabilityHealth::Available,
            DEFAULT_IN_FLIGHT_LEASE_MS + 2,
        ),
        CompletionVerdict::Rejected(CompletionRejection::InactiveOwner)
    );
    assert_eq!(
        scheduler.scheduling_snapshot(),
        before_unknown_completion,
        "another request cannot retire a stalled target or release its scope"
    );
    assert!(!scheduler.reserve_submission_with_tracking(
        &CapabilityId::TELEMETRY_CPU,
        request_id(51),
        DEFAULT_IN_FLIGHT_LEASE_MS + 1,
        target("disk:slow"),
    ));
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
    assert_eq!(scheduler.target_job_count(), 0);
    let snapshot = scheduler.scheduling_snapshot();
    assert_eq!(snapshot.active_stalled_targets, 0);
    assert_eq!(snapshot.recovered_stalls, 1);
    assert_eq!(snapshot.target_recovered_stalls, 1);
    assert_eq!(snapshot.budgets.pending_deliveries, 0);
}
