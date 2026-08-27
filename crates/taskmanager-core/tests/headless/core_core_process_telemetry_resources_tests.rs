use super::*;

fn observations() -> ProcessResourceObservations {
    ProcessResourceObservations {
        limits: ResourceObservation::current(Vec::new(), 10),
        resource_groups: ResourceObservation::current(Vec::new(), 10),
        memory_usage_bytes: ResourceObservation::current(0, 10),
        memory_limit: ResourceObservation::current(LimitValue::Unlimited, 10),
        cpu_time_quota_micros: ResourceObservation::current(LimitValue::Unlimited, 10),
        cpu_time_period_micros: ResourceObservation::current(0, 10),
        process_count: ResourceObservation::current(0, 10),
        process_limit: ResourceObservation::current(LimitValue::Unlimited, 10),
    }
}

fn snapshot(observations: ProcessResourceObservations) -> ProcessResourceSnapshot {
    ProcessResourceSnapshot::from_observations(DeviceState::healthy(10), observations, Vec::new())
}

#[test]
fn zero_and_unlimited_are_current_serializable_values() {
    let zero = ResourceObservation::current(0_u64, 42);
    let unlimited = ResourceObservation::current(LimitValue::Unlimited, 42);

    assert_eq!(zero.current_value(), Some(&0));
    assert_eq!(zero.last_success_ms(), Some(42));
    assert_eq!(unlimited.current_value(), Some(&LimitValue::Unlimited));
    assert_eq!(unlimited.last_success_ms(), Some(42));
    assert_eq!(
        serde_json::to_value(unlimited).expect("unlimited observation serializes"),
        serde_json::json!({
            "status": "current",
            "value": "unlimited",
            "observed_at_ms": 42
        })
    );
}

#[test]
fn absent_stale_and_unavailable_are_structurally_distinct() {
    let absent = ResourceObservation::<u64>::absent(10);
    let stale = absent
        .clone()
        .transition_failure(FailureKind::IdentityChanged);
    let unavailable = ResourceObservation::<u64>::unavailable(FailureKind::IdentityChanged);

    assert!(absent.is_current());
    assert_eq!(absent.current_value(), None);
    assert_eq!(absent.last_success_ms(), Some(10));
    assert!(matches!(
        stale,
        ResourceObservation::Stale {
            last: ResourceLastObservation::Absent,
            last_success_ms: 10,
            failure: FailureKind::IdentityChanged
        }
    ));
    assert_eq!(unavailable.last_success_ms(), None);
}

#[test]
fn legacy_only_payload_hydrates_canonical_observations() {
    let snapshot: ProcessResourceSnapshot = serde_json::from_value(serde_json::json!({
        "state": {"status": "healthy", "last_success_ms": 10},
        "limits": [{
            "kind": "open_files",
            "soft": {"value": 1024},
            "hard": "unlimited",
            "unit": "files"
        }],
        "groups": [{
            "provider": "fixture.group",
            "hierarchy_id": 0,
            "controllers": ["memory"],
            "path": "/fixture"
        }],
        "memory_current_bytes": 0,
        "memory_max": "unlimited",
        "pids_current": 3
    }))
    .expect("schema-v1 payload remains readable");

    assert_eq!(snapshot.current_memory_usage_bytes(), Some(0));
    assert_eq!(snapshot.current_memory_limit(), Some(LimitValue::Unlimited));
    assert_eq!(snapshot.current_process_count(), Some(3));
    assert_eq!(snapshot.current_limits().unwrap().len(), 1);
    assert_eq!(
        snapshot.current_resource_groups().unwrap()[0]
            .provider
            .as_str(),
        "fixture.group"
    );
    assert!(matches!(
        &snapshot.observations().memory_usage_bytes,
        ResourceObservation::Current {
            value: 0,
            observed_at_ms: 10
        }
    ));
}

#[test]
fn typed_truth_wins_over_conflicting_legacy_values() {
    let snapshot: ProcessResourceSnapshot = serde_json::from_value(serde_json::json!({
        "state": {"status": "healthy", "last_success_ms": 10},
        "memory_current_bytes": 999,
        "memory_max": {"value": 2048},
        "observations": {
            "memory_usage_bytes": {
                "status": "current", "value": 64, "observed_at_ms": 11
            },
            "memory_limit": {
                "status": "unavailable", "failure": "permission_denied"
            }
        }
    }))
    .expect("mixed compatibility payload decodes");

    assert_eq!(snapshot.current_memory_usage_bytes(), Some(64));
    assert_eq!(snapshot.current_memory_limit(), None);

    let encoded = serde_json::to_value(&snapshot).expect("typed truth serializes");
    assert_eq!(encoded["memory_current_bytes"], 64);
    assert_eq!(encoded["memory_max"], serde_json::Value::Null);
}

#[test]
fn typed_partial_or_stale_values_do_not_project_as_legacy_success() {
    let mut typed = observations();
    typed.memory_usage_bytes =
        ResourceObservation::partial(64, 10, FailureKind::TemporarilyUnavailable);
    typed.memory_limit = ResourceObservation::current(LimitValue::Unlimited, 10)
        .transition_failure(FailureKind::ProviderFault);
    let snapshot = snapshot(typed);

    assert_eq!(snapshot.current_memory_usage_bytes(), Some(64));
    assert_eq!(snapshot.current_memory_limit(), None);
    let encoded = serde_json::to_value(snapshot).expect("typed degradation serializes");
    assert_eq!(encoded["memory_current_bytes"], serde_json::Value::Null);
    assert_eq!(encoded["memory_max"], serde_json::Value::Null);
}

#[test]
fn unknown_empty_legacy_lists_are_not_authoritative_empty_observations() {
    let snapshot: ProcessResourceSnapshot = serde_json::from_value(serde_json::json!({
        "state": {"status": "healthy", "last_success_ms": 7},
        "limits": [],
        "groups": []
    }))
    .expect("empty legacy lists decode");

    assert_eq!(snapshot.current_limits(), None);
    assert_eq!(snapshot.current_resource_groups(), None);
}

#[test]
fn legacy_values_without_a_success_time_do_not_fabricate_current_truth() {
    let snapshot: ProcessResourceSnapshot = serde_json::from_value(serde_json::json!({
        "memory_current_bytes": 4096,
        "pids_current": 7
    }))
    .expect("legacy payload without health metadata still decodes");

    assert_eq!(snapshot.current_memory_usage_bytes(), None);
    assert_eq!(snapshot.current_process_count(), None);
    assert!(matches!(
        &snapshot.observations().memory_usage_bytes,
        ResourceObservation::Unknown
    ));
}

#[test]
fn typed_confirmed_empty_does_not_depend_on_a_legacy_vec_sentinel() {
    let typed = ProcessResourceObservations {
        limits: ResourceObservation::absent(7),
        resource_groups: ResourceObservation::current(Vec::new(), 7),
        ..ProcessResourceObservations::default()
    };
    let snapshot = snapshot(typed);

    assert_eq!(snapshot.current_limits(), Some([].as_slice()));
    assert_eq!(snapshot.current_resource_groups(), Some([].as_slice()));
    let round_trip: ProcessResourceSnapshot =
        serde_json::from_value(serde_json::to_value(snapshot).expect("confirmed empty serializes"))
            .expect("confirmed empty round trips");
    assert!(matches!(
        &round_trip.observations().limits,
        ResourceObservation::Absent { observed_at_ms: 7 }
    ));
    assert!(matches!(
        &round_trip.observations().resource_groups,
        ResourceObservation::Current { value, .. } if value.is_empty()
    ));
}

#[test]
fn changed_group_blocks_cgroup_stale_but_not_independent_rlimit_stale() {
    let previous = snapshot(observations());

    let failed = snapshot(ProcessResourceObservations {
        limits: ResourceObservation::unavailable(FailureKind::PermissionDenied),
        resource_groups: ResourceObservation::current(Vec::new(), 20),
        memory_usage_bytes: ResourceObservation::unavailable(FailureKind::ProviderFault),
        memory_limit: ResourceObservation::unavailable(FailureKind::ProviderFault),
        cpu_time_quota_micros: ResourceObservation::unavailable(FailureKind::ProviderFault),
        cpu_time_period_micros: ResourceObservation::unavailable(FailureKind::ProviderFault),
        process_count: ResourceObservation::unavailable(FailureKind::ProviderFault),
        process_limit: ResourceObservation::unavailable(FailureKind::ProviderFault),
    });

    let retained = failed.retain_previous(previous, false);
    assert!(matches!(
        &retained.observations().limits,
        ResourceObservation::Stale {
            last_success_ms: 10,
            failure: FailureKind::PermissionDenied,
            ..
        }
    ));
    assert!(matches!(
        &retained.observations().memory_usage_bytes,
        ResourceObservation::Unavailable {
            failure: FailureKind::ProviderFault
        }
    ));
    assert_eq!(retained.current_memory_usage_bytes(), None);
}
