use taskmanager_core::{
    DeviceState, DeviceStatus, FailureKind, StartupBootEvidenceSnapshot, StartupCriticalChainNode,
    StartupEvidenceFailure, StartupFailedUnit,
};

use super::{
    StartupEvidenceProjection, StartupEvidenceProjectionApplyResult, StartupEvidenceRevision,
    StartupEvidenceUnavailable,
};

fn current(now_ms: u64) -> StartupBootEvidenceSnapshot {
    StartupBootEvidenceSnapshot {
        state: DeviceState::healthy(now_ms),
        failed_units_state: DeviceState::healthy(now_ms),
        critical_chain_state: DeviceState::healthy(now_ms),
        failed_units_failure: None,
        critical_chain_failure: None,
        failed_units: vec![StartupFailedUnit {
            unit: "broken.service".into(),
            load_state: "loaded".into(),
            active_state: "failed".into(),
            sub_state: "failed".into(),
            description: "Broken worker".into(),
        }],
        critical_chain: vec![StartupCriticalChainNode {
            unit: "worker.service".into(),
            activated_at_ms: Some(900),
            duration_ms: Some(300),
        }],
    }
}

#[test]
fn provider_failure_retains_last_successful_values_as_typed_stale() {
    let revision = StartupEvidenceRevision::new(1);
    let mut projection = StartupEvidenceProjection::default();
    projection.begin(revision);
    assert!(matches!(
        projection.apply(revision, current(10), 10),
        StartupEvidenceProjectionApplyResult::Applied(_)
    ));

    let StartupEvidenceProjectionApplyResult::Applied(projected) = projection.apply_failure(
        revision,
        StartupEvidenceUnavailable::Provider(FailureKind::TimedOut),
        20,
    ) else {
        panic!("current revision should accept the provider failure");
    };
    assert_eq!(projected.snapshot.failed_units.len(), 1);
    assert_eq!(projected.snapshot.critical_chain.len(), 1);
    assert_eq!(
        projected.snapshot.failed_units_state.status,
        DeviceStatus::Stale
    );
    assert_eq!(
        projected.snapshot.failed_units_state.last_success_ms,
        Some(10)
    );
    assert_eq!(
        projected.snapshot.failed_units_failure,
        Some(StartupEvidenceFailure::TimedOut)
    );
}

#[test]
fn failed_subsource_refresh_retains_only_that_sources_last_successful_values() {
    let revision = StartupEvidenceRevision::new(1);
    let mut projection = StartupEvidenceProjection::default();
    projection.begin(revision);
    let _ = projection.apply(revision, current(10), 10);
    let incoming = StartupBootEvidenceSnapshot {
        state: DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: None,
        },
        failed_units_state: DeviceState::healthy(20),
        critical_chain_state: DeviceState {
            status: DeviceStatus::Stale,
            last_success_ms: None,
        },
        failed_units_failure: None,
        critical_chain_failure: Some(StartupEvidenceFailure::TimedOut),
        failed_units: Vec::new(),
        critical_chain: Vec::new(),
    };

    let StartupEvidenceProjectionApplyResult::Applied(projected) =
        projection.apply(revision, incoming, 20)
    else {
        panic!("current revision should be applied");
    };
    assert!(projected.snapshot.failed_units.is_empty());
    assert_eq!(projected.snapshot.critical_chain.len(), 1);
    assert_eq!(
        projected.snapshot.critical_chain_state.last_success_ms,
        Some(10)
    );
}

#[test]
fn older_completion_cannot_overwrite_a_newer_submitted_revision() {
    let mut projection = StartupEvidenceProjection::default();
    let old = StartupEvidenceRevision::new(1);
    let current_revision = StartupEvidenceRevision::new(2);
    projection.begin(old);
    let _ = projection.apply(old, current(10), 10);
    projection.begin(current_revision);

    assert!(matches!(
        projection.apply(old, current(30), 30),
        StartupEvidenceProjectionApplyResult::Ignored(_)
    ));
    let snapshot = projection.snapshot().expect("active projection");
    assert_eq!(snapshot.revision, current_revision);
    assert_eq!(
        snapshot.snapshot.failed_units_state.last_success_ms,
        Some(10)
    );
}

#[test]
fn recovery_replaces_retained_values_and_clears_transport_failure() {
    let revision = StartupEvidenceRevision::new(1);
    let mut projection = StartupEvidenceProjection::default();
    projection.begin(revision);
    let _ = projection.apply(revision, current(10), 10);
    let _ = projection.apply_failure(
        revision,
        StartupEvidenceUnavailable::Provider(FailureKind::PermissionDenied),
        20,
    );

    let StartupEvidenceProjectionApplyResult::Applied(recovered) =
        projection.apply(revision, current(30), 30)
    else {
        panic!("recovery should apply");
    };
    assert_eq!(recovered.unavailable, None);
    assert_eq!(
        recovered.snapshot.critical_chain_state.status,
        DeviceStatus::Healthy
    );
    assert_eq!(
        recovered.snapshot.critical_chain_state.last_success_ms,
        Some(30)
    );
}
