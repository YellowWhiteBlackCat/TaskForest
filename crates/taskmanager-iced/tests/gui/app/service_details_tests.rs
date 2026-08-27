use super::*;

fn dependencies(kind: ServiceRelationKind, target: &str) -> ServiceDeps {
    let mut dependencies = ServiceDeps::default();
    dependencies.replace_relation_targets(kind, [ServiceId::new(target)]);
    dependencies
}

#[test]
fn details_state_discards_updates_for_stale_service_identity() {
    let mut state = ServiceDetailsState::default();
    let active = ServiceId::new("systemd:demo.service");
    let stale = ServiceId::new("systemd:old.service");
    assert!(state.select(&active));
    let mut lifecycle = ServiceDependenciesLifecycle::default();
    lifecycle.begin(RequestId::MIN, active.clone());

    assert!(!lifecycle.resolve(
        RequestId::MIN,
        stale,
        dependencies(ServiceRelationKind::Requires, "wrong.target"),
    ));
    assert!(state.snapshot(&lifecycle, 0).dependencies.is_loading());

    assert!(lifecycle.resolve(
        RequestId::MIN,
        active,
        dependencies(ServiceRelationKind::Requires, "network.target"),
    ));
    assert_eq!(
        state
            .snapshot(&lifecycle, 0)
            .dependencies
            .projected()
            .expect("ready dependencies")
            .relation_projection(&ServiceRelationKind::Requires),
        "network.target"
    );
    assert!(!state.snapshot(&lifecycle, 0).dependencies.is_loading());
}

#[test]
fn details_state_exposes_typed_failure_and_retry_resets_it() {
    let mut state = ServiceDetailsState::default();
    let service_id = ServiceId::new("systemd:missing.service");
    assert!(state.select(&service_id));
    let mut lifecycle = ServiceDependenciesLifecycle::default();
    lifecycle.begin(RequestId::MIN, service_id.clone());
    assert!(lifecycle.fail(
        RequestId::MIN,
        service_id.clone(),
        FailureKind::PermissionDenied,
    ));

    let failed = state.snapshot(&lifecycle, 0);
    assert_eq!(
        failed.dependencies.failure(),
        Some(FailureKind::PermissionDenied)
    );
    assert!(!failed.dependencies.is_loading());
    assert_eq!(state.begin_refresh(), Some(service_id));
    lifecycle.begin(
        RequestId::new(2).expect("fixture id"),
        ServiceId::new("systemd:missing.service"),
    );
    let loading = state.snapshot(&lifecycle, 0);
    assert!(loading.dependencies.is_loading());
    assert_eq!(loading.dependencies.failure(), None);
}
