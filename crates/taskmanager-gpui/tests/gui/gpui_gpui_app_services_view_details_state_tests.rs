use super::*;

fn dependencies(kind: ServiceRelationKind, target: &str) -> ServiceDeps {
    let mut dependencies = ServiceDeps::default();
    dependencies.replace_relation_targets(kind, [ServiceId::new(target)]);
    dependencies
}

#[test]
fn details_state_projects_only_the_shared_correlated_dependency_session() {
    let mut state = ServiceDetailsState::new();
    let service_id = ServiceId::new("fixture.service:application-port-active");
    assert!(state.select(&service_id));
    let mut lifecycle = ServiceDependenciesLifecycle::default();
    lifecycle.begin(RequestId::MIN, service_id.clone());
    assert!(!lifecycle.resolve(
        RequestId::MIN,
        ServiceId::new("fixture.service:stale-selection"),
        dependencies(ServiceRelationKind::Requires, "wrong.target"),
    ));
    assert!(lifecycle.resolve(
        RequestId::MIN,
        service_id.clone(),
        dependencies(ServiceRelationKind::Requires, "network.target"),
    ));
    let log_request_id = RequestId::new(1).expect("fixture id");
    assert!(state.accept_log_snapshot(&service_id, log_request_id));
    state.apply(ServiceUpdate::Logs {
        request_id: log_request_id,
        snapshot: taskmanager_core::core::services::ServiceLogSnapshot {
            service_id: service_id.clone(),
            state: ServiceLogState::from_lines(vec!["ready".into()]),
        },
    });

    let snapshot = state.details_for(&service_id, &lifecycle);
    assert_eq!(
        snapshot
            .dependencies
            .projected()
            .expect("dependency request is ready")
            .relation_projection(&ServiceRelationKind::Requires),
        "network.target"
    );
    assert_eq!(
        snapshot.logs,
        ServiceLogState::from_lines(vec!["ready".into()])
    );
}

#[test]
fn late_log_snapshot_cannot_replace_a_newer_details_request() {
    let mut state = ServiceDetailsState::new();
    let service_id = ServiceId::new("fixture.service:application-port-snapshot-race");
    assert!(state.select(&service_id));

    let first_request = RequestId::new(10).expect("fixture id");
    assert!(state.accept_log_snapshot(&service_id, first_request));
    state.apply(ServiceUpdate::Logs {
        request_id: first_request,
        snapshot: taskmanager_core::core::services::ServiceLogSnapshot {
            service_id: service_id.clone(),
            state: ServiceLogState::from_lines(vec!["first".into()]),
        },
    });

    assert!(state.begin_log_refresh(&service_id));
    let second_request = RequestId::new(11).expect("fixture id");
    assert!(state.accept_log_snapshot(&service_id, second_request));

    state.apply(ServiceUpdate::Logs {
        request_id: first_request,
        snapshot: taskmanager_core::core::services::ServiceLogSnapshot {
            service_id: service_id.clone(),
            state: ServiceLogState::from_lines(vec!["stale".into()]),
        },
    });
    assert_eq!(
        state
            .snapshot(&ServiceDependenciesLifecycle::default())
            .logs,
        ServiceLogState::Loading
    );

    state.apply(ServiceUpdate::Logs {
        request_id: second_request,
        snapshot: taskmanager_core::core::services::ServiceLogSnapshot {
            service_id,
            state: ServiceLogState::from_lines(vec!["second".into()]),
        },
    });
    assert_eq!(
        state
            .snapshot(&ServiceDependenciesLifecycle::default())
            .logs,
        ServiceLogState::from_lines(vec!["second".into()])
    );
}

#[test]
fn rejected_stream_attempt_becomes_typed_unavailable_state() {
    let mut state = ServiceDetailsState::new();
    let service_id = ServiceId::new("fixture.service:application-port-rejected");
    assert!(state.select(&service_id));
    let query = state
        .next_follow_request(&service_id, 1_000)
        .expect("initial follow query");
    let attempt_id = state
        .begin_stream_attempt(query)
        .expect("targeted attempt starts");
    state.reject_stream(
        attempt_id,
        taskmanager_core::core::failure::FailureKind::TemporarilyUnavailable,
    );

    assert!(matches!(
        state
            .snapshot(&ServiceDependenciesLifecycle::default())
            .log_stream,
        ServiceLogStreamState::Unavailable(ServiceLogFailure {
            kind: ServiceLogErrorKind::TemporarilyUnavailable,
            ..
        })
    ));
}

#[test]
fn shared_dependency_failure_and_retry_keep_one_typed_authority() {
    let mut state = ServiceDetailsState::new();
    let service_id = ServiceId::new("fixture.service:application-port-detail-rejected");
    assert!(state.select(&service_id));
    let mut lifecycle = ServiceDependenciesLifecycle::default();
    lifecycle.begin(RequestId::MIN, service_id.clone());
    assert!(lifecycle.fail(
        RequestId::MIN,
        service_id.clone(),
        taskmanager_core::core::failure::FailureKind::Rejected,
    ));
    let log_request_id = RequestId::new(20).expect("fixture id");
    assert!(state.accept_log_snapshot(&service_id, log_request_id));
    state.apply(ServiceUpdate::Logs {
        request_id: log_request_id,
        snapshot: taskmanager_core::core::services::ServiceLogSnapshot {
            service_id: service_id.clone(),
            state: ServiceLogState::Unavailable(ServiceLogFailure::with_detail(
                ServiceLogErrorKind::ProviderFailed,
                "queue rejected",
            )),
        },
    });

    let snapshot = state.snapshot(&lifecycle);
    assert_eq!(
        snapshot.dependencies.failure(),
        Some(taskmanager_core::core::failure::FailureKind::Rejected)
    );
    assert!(matches!(
        snapshot.logs,
        ServiceLogState::Unavailable(ServiceLogFailure {
            kind: ServiceLogErrorKind::ProviderFailed,
            ..
        })
    ));
    lifecycle.begin(RequestId::new(2).expect("fixture id"), service_id.clone());
    assert!(state.snapshot(&lifecycle).dependencies.is_loading());
}

#[test]
fn replacing_provider_target_drops_previous_dependency_generation() {
    let mut state = ServiceDetailsState::new();
    let systemd_id = ServiceId::new("linux.service.systemd:demo.service");
    let openrc_id = ServiceId::new("linux.service.openrc:demo");
    let mut lifecycle = ServiceDependenciesLifecycle::default();

    assert!(state.select(&systemd_id));
    lifecycle.begin(RequestId::MIN, systemd_id.clone());
    assert!(lifecycle.resolve(
        RequestId::MIN,
        systemd_id,
        dependencies(ServiceRelationKind::Requires, "systemd.target"),
    ));

    assert!(state.select(&openrc_id));
    lifecycle.begin(RequestId::new(2).expect("fixture id"), openrc_id);
    let snapshot = state.snapshot(&lifecycle);
    assert!(snapshot.dependencies.projected().is_none());
    assert!(snapshot.dependencies.is_loading());
}
