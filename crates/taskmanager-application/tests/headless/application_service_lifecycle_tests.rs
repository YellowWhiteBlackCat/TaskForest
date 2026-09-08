use taskmanager_application::{
    ServiceDependenciesLifecycle, ServiceLifecycleState, ServiceLogStreamLifecycle,
    ServiceRequestCorrelation,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::services::{
    ServiceAction, ServiceDeps, ServiceLogEntries, ServiceLogEntry, ServiceLogErrorKind,
    ServiceLogFailure, ServiceLogLevel, ServiceLogLevelFilter, ServiceLogQuery,
    ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter, ServiceRelationKind,
    ServiceStatus,
};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::{RequestId, SubmissionErrorKind};

fn request(value: u64) -> RequestId {
    RequestId::new(value).expect("test request id is nonzero")
}

fn dependencies(target: &str) -> ServiceDeps {
    let mut deps = ServiceDeps::default();
    deps.replace_relation_targets(ServiceRelationKind::Requires, [ServiceId::new(target)]);
    deps
}

fn query(
    service_id: &ServiceId,
    level: ServiceLogLevelFilter,
    cursor: Option<&str>,
) -> ServiceLogQuery {
    ServiceLogQuery {
        service_id: service_id.clone(),
        level,
        time: ServiceLogTimeFilter::All,
        after_cursor: cursor.map(str::to_owned),
    }
}

fn ready(query: ServiceLogQuery, cursor: &str) -> ServiceLogStreamSnapshot {
    ServiceLogStreamSnapshot {
        query,
        state: ServiceLogStreamState::Ready(
            ServiceLogEntries::new(vec![ServiceLogEntry {
                cursor: cursor.into(),
                realtime_timestamp_micros: None,
                priority: None,
                level: ServiceLogLevel::Info,
                message: format!("entry {cursor}"),
            }])
            .expect("fixture has one entry"),
        ),
    }
}

#[test]
fn dependency_lifecycle_rejects_late_duplicate_and_closed_completions() {
    let service = ServiceId::new("systemd:demo.service");
    let mut lifecycle = ServiceDependenciesLifecycle::default();

    lifecycle.begin(request(1), service.clone());
    assert!(lifecycle.resolve(request(1), service.clone(), dependencies("first.target")));
    lifecycle.begin(request(2), service.clone());
    assert!(lifecycle.is_loading());
    assert_eq!(
        lifecycle
            .projected()
            .expect("refresh retains last good")
            .relation_projection(&ServiceRelationKind::Requires),
        "first.target"
    );
    assert!(!lifecycle.resolve(request(1), service.clone(), dependencies("late.target")));
    assert!(lifecycle.fail(request(2), service.clone(), FailureKind::PermissionDenied));
    assert!(!lifecycle.fail(request(2), service.clone(), FailureKind::Rejected));

    lifecycle.begin(request(3), service.clone());
    assert_eq!(
        lifecycle
            .projected()
            .expect("retry retains last good")
            .relation_projection(&ServiceRelationKind::Requires),
        "first.target"
    );
    lifecycle.close();
    assert!(!lifecycle.resolve(request(3), service.clone(), dependencies("closed.target")));
    assert!(lifecycle.projected().is_none());

    lifecycle.begin(request(4), service.clone());
    assert!(lifecycle.resolve(request(4), service, dependencies("reopened.target")));
    assert_eq!(
        lifecycle
            .projected()
            .expect("reopen accepts only its request")
            .relation_projection(&ServiceRelationKind::Requires),
        "reopened.target"
    );
}

#[test]
fn log_lifecycle_correlates_request_filter_cursor_failure_and_retry() {
    let service = ServiceId::new("systemd:demo.service");
    let initial = query(&service, ServiceLogLevelFilter::All, None);
    let filtered = query(&service, ServiceLogLevelFilter::Errors, Some("cursor-1"));
    let mut lifecycle = ServiceLogStreamLifecycle::open(service.clone());

    assert!(lifecycle.begin(request(1), initial.clone()));
    assert!(lifecycle.begin(request(2), filtered.clone()));
    assert!(!lifecycle.resolve(request(1), ready(initial, "stale")));
    assert!(lifecycle.resolve(request(2), ready(filtered.clone(), "current")));
    assert!(!lifecycle.resolve(request(2), ready(filtered.clone(), "duplicate")));

    assert!(lifecycle.begin(request(3), filtered.clone()));
    assert!(lifecycle.resolve(
        request(3),
        ServiceLogStreamSnapshot {
            query: filtered.clone(),
            state: ServiceLogStreamState::Unavailable(ServiceLogFailure::with_detail(
                ServiceLogErrorKind::PermissionDenied,
                "denied",
            )),
        }
    ));
    assert_eq!(
        lifecycle.failure().map(|failure| failure.kind),
        Some(ServiceLogErrorKind::PermissionDenied)
    );
    assert!(lifecycle.begin(request(4), filtered.clone()));
    assert!(matches!(
        lifecycle.projected_state(),
        ServiceLogStreamState::Ready(_)
    ));
    lifecycle.close();
    assert!(!lifecycle.resolve(request(4), ready(filtered, "closed")));
}

#[test]
fn admission_attempt_identity_and_filter_generation_are_not_implicit() {
    assert_eq!(
        taskmanager_application::service_submission_failure(SubmissionErrorKind::Busy),
        FailureKind::TemporarilyUnavailable
    );
    assert_eq!(
        taskmanager_application::service_submission_failure(
            SubmissionErrorKind::UnsupportedCapability
        ),
        FailureKind::Unsupported
    );
    assert_eq!(
        taskmanager_application::service_submission_failure(SubmissionErrorKind::InvalidRequest),
        FailureKind::Rejected
    );
    let service = ServiceId::new("systemd:demo.service");
    let mut dependencies = ServiceDependenciesLifecycle::default();
    let abandoned = dependencies.begin_attempt(service.clone());
    let current = dependencies.begin_attempt(service.clone());
    assert!(!dependencies.reject_attempt(abandoned, FailureKind::Rejected));
    assert!(dependencies.reject_attempt(current, FailureKind::Rejected));
    assert!(matches!(
        dependencies,
        ServiceDependenciesLifecycle::Failed {
            correlation: ServiceRequestCorrelation::Attempt(id),
            ..
        } if id == current
    ));

    let all = query(&service, ServiceLogLevelFilter::All, None);
    let errors = query(&service, ServiceLogLevelFilter::Errors, None);
    let mut logs = ServiceLogStreamLifecycle::open(service);
    logs.begin(request(10), all.clone());
    assert!(logs.resolve(request(10), ready(all, "retained")));
    let attempt = logs
        .begin_attempt(errors.clone())
        .expect("same target, new filter generation");
    assert!(matches!(
        logs.projected_state(),
        ServiceLogStreamState::Loading
    ));
    assert!(logs.accept_attempt(attempt, request(11)));
    assert!(!logs.resolve(
        request(10),
        ready(
            query(&errors.service_id, ServiceLogLevelFilter::All, None),
            "late"
        )
    ));
    assert!(logs.resolve(request(11), ready(errors, "filtered")));
}

#[test]
fn service_lifecycle_state_transitions_follow_canonical_matrix() {
    use ServiceLifecycleState::*;

    // Reflexive transitions (heartbeat/poll)
    for state in [Active, Inactive, Failed, Reloading] {
        assert!(state.can_transition_to(state));
    }

    // Inactive transitions
    assert!(Inactive.can_transition_to(Active));
    assert!(Inactive.can_transition_to(Failed));
    assert!(!Inactive.can_transition_to(Reloading));

    // Active transitions
    assert!(Active.can_transition_to(Inactive));
    assert!(Active.can_transition_to(Reloading));
    assert!(Active.can_transition_to(Failed));

    // Reloading transitions
    assert!(Reloading.can_transition_to(Active));
    assert!(Reloading.can_transition_to(Failed));
    assert!(Reloading.can_transition_to(Inactive));

    // Failed transitions
    assert!(Failed.can_transition_to(Active));
    assert!(Failed.can_transition_to(Inactive));
    assert!(!Failed.can_transition_to(Reloading));

    // Action applicability
    assert!(Inactive.is_action_applicable(ServiceAction::Start));
    assert!(!Inactive.is_action_applicable(ServiceAction::Stop));
    assert!(Inactive.is_action_applicable(ServiceAction::Restart));

    assert!(!Active.is_action_applicable(ServiceAction::Start));
    assert!(Active.is_action_applicable(ServiceAction::Stop));
    assert!(Active.is_action_applicable(ServiceAction::Restart));

    assert!(Reloading.is_action_applicable(ServiceAction::Stop));
    assert!(Failed.is_action_applicable(ServiceAction::Start));
    assert!(Failed.is_action_applicable(ServiceAction::Stop));
}

#[test]
fn service_lifecycle_state_provider_classification_and_conversions() {
    use ServiceLifecycleState::*;

    // Provider state classification
    assert_eq!(
        ServiceLifecycleState::from_provider_states("active", "running"),
        Active
    );
    assert_eq!(
        ServiceLifecycleState::from_provider_states("active", "reloading"),
        Reloading
    );
    assert_eq!(
        ServiceLifecycleState::from_provider_states("active", "failed"),
        Failed
    );
    assert_eq!(
        ServiceLifecycleState::from_provider_states("inactive", "dead"),
        Inactive
    );
    assert_eq!(
        ServiceLifecycleState::from_provider_states("failed", "failed"),
        Failed
    );

    // From string representations
    assert_eq!(ServiceLifecycleState::from("active"), Active);
    assert_eq!(ServiceLifecycleState::from("running"), Active);
    assert_eq!(ServiceLifecycleState::from("reloading"), Reloading);
    assert_eq!(ServiceLifecycleState::from("inactive"), Inactive);
    assert_eq!(ServiceLifecycleState::from("failed"), Failed);
    assert_eq!(ServiceLifecycleState::from("stopped"), Inactive);

    // Conversions to and from ServiceStatus
    assert_eq!(ServiceLifecycleState::from(ServiceStatus::Active), Active);
    assert_eq!(
        ServiceLifecycleState::from(ServiceStatus::Inactive),
        Inactive
    );
    assert_eq!(ServiceLifecycleState::from(ServiceStatus::Failed), Failed);
    assert_eq!(
        ServiceLifecycleState::from(ServiceStatus::Unknown),
        Inactive
    );

    assert_eq!(ServiceStatus::from(Active), ServiceStatus::Active);
    assert_eq!(ServiceStatus::from(Reloading), ServiceStatus::Active);
    assert_eq!(ServiceStatus::from(Inactive), ServiceStatus::Inactive);
    assert_eq!(ServiceStatus::from(Failed), ServiceStatus::Failed);

    // Queries and displays
    assert!(Active.is_operational());
    assert!(Reloading.is_operational());
    assert!(!Inactive.is_operational());
    assert!(!Failed.is_operational());

    assert_eq!(Active.as_str(), "Active");
    assert_eq!(Inactive.as_str(), "Inactive");
    assert_eq!(Failed.as_str(), "Failed");
    assert_eq!(Reloading.as_str(), "Reloading");
    assert_eq!(format!("{Reloading}"), "Reloading");
}
