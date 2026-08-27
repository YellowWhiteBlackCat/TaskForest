use super::*;
use crate::{ProcessInsightSnapshot, ProcessInsightsRevision};
use taskmanager_core::{
    DeviceState, DeviceStatus, ProcessResourceObservations, ProcessThreadInfo, ThreadState,
};

fn target(start_time_secs: u64) -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(42, "worker", start_time_secs, 9_000)
        .expect("fixture identity")
}

fn network(
    target: FrozenProcessIdentity,
    revision: u64,
    raw_start: u64,
) -> ProcessInsightFacetEvent {
    ProcessInsightFacetEvent::Network(Box::new(ProcessInsightObservation {
        target,
        revision: ProcessInsightsRevision::new(revision),
        snapshot: ProcessInsightSnapshot {
            identity: ProcessIdentity {
                pid: 42,
                start_token: raw_start,
            },
            value: ProcessNetworkSnapshot::default(),
        },
    }))
}

#[test]
fn first_facet_updates_a_typed_partial_projection_immediately() {
    let mut projection = ProcessInsightsProjection::default();
    projection.begin(target(100), ProcessInsightsRevision::new(1));

    let result = projection.apply(&network(target(100), 1, 9_000));

    assert!(matches!(
        result,
        ProcessInsightsProjectionApplyResult::AppliedPartial(_)
    ));
    assert!(matches!(
        projection.current().map(|current| &current.network),
        Some(ProcessInsightFacetState::Current(_))
    ));
    assert!(matches!(
        projection.current().map(|current| &current.gpu),
        Some(ProcessInsightFacetState::Pending)
    ));
}

#[test]
fn threads_facet_preserves_identity_bound_cpu_percent() {
    let selected = target(100);
    let revision = ProcessInsightsRevision::new(1);
    let mut projection = ProcessInsightsProjection::default();
    projection.begin(selected.clone(), revision);

    let event = ProcessInsightFacetEvent::Threads(Box::new(ProcessInsightObservation {
        target: selected,
        revision,
        snapshot: ProcessInsightSnapshot {
            identity: ProcessIdentity {
                pid: 42,
                start_token: 9_000,
            },
            value: ProcessThreads {
                state: DeviceState::healthy(10),
                threads: vec![ProcessThreadInfo {
                    tid: 43,
                    comm: "worker".to_owned(),
                    state: ThreadState::Running,
                    cpu_time_secs: Some(2.0),
                    cpu_percent: Some(37.5),
                }],
            },
        },
    }));

    assert!(matches!(
        projection.apply(&event),
        ProcessInsightsProjectionApplyResult::AppliedPartial(_)
    ));
    let current = projection.current().expect("thread projection");
    let ProcessInsightFacetState::Current(threads) = &current.threads else {
        panic!("thread observation must remain current");
    };
    assert_eq!(threads.threads[0].cpu_percent, Some(37.5));
}

#[test]
fn stale_revision_and_different_frozen_generation_cannot_pollute_selection() {
    let mut projection = ProcessInsightsProjection::default();
    projection.begin(target(200), ProcessInsightsRevision::new(2));

    assert_eq!(
        projection.apply(&network(target(200), 1, 9_000)),
        ProcessInsightsProjectionApplyResult::Ignored(
            ProcessInsightsProjectionRejection::StaleOrUnexpectedRevision
        )
    );
    assert_eq!(
        projection.apply(&network(target(100), 2, 9_000)),
        ProcessInsightsProjectionApplyResult::Ignored(
            ProcessInsightsProjectionRejection::DifferentFrozenTarget
        )
    );
    assert!(matches!(
        projection.current().map(|current| &current.network),
        Some(ProcessInsightFacetState::Pending)
    ));
}

#[test]
fn provider_raw_identity_must_match_across_domains_not_frozen_wall_clock() {
    let mut projection = ProcessInsightsProjection::default();
    projection.begin(target(1_720_000_000), ProcessInsightsRevision::new(1));
    assert!(matches!(
        projection.apply(&network(target(1_720_000_000), 1, 9_000)),
        ProcessInsightsProjectionApplyResult::AppliedPartial(_)
    ));
    let gpu = ProcessInsightFacetEvent::Gpu(Box::new(ProcessInsightObservation {
        target: target(1_720_000_000),
        revision: ProcessInsightsRevision::new(1),
        snapshot: ProcessInsightSnapshot {
            identity: ProcessIdentity {
                pid: 42,
                start_token: 9_001,
            },
            value: ProcessGpuSnapshot::default(),
        },
    }));
    assert_eq!(
        projection.apply(&gpu),
        ProcessInsightsProjectionApplyResult::Ignored(
            ProcessInsightsProjectionRejection::ConflictingRawIdentity
        )
    );
}

#[test]
fn terminal_partial_keeps_current_domains_renderable_and_failed_domain_unavailable() {
    let selected = target(100);
    let revision = ProcessInsightsRevision::new(1);
    let identity = ProcessIdentity {
        pid: 42,
        start_token: 9_000,
    };
    let mut projection = ProcessInsightsProjection::default();
    projection.begin(selected.clone(), revision);
    let network = ProcessInsightFacetEvent::Network(Box::new(ProcessInsightObservation {
        target: selected.clone(),
        revision,
        snapshot: ProcessInsightSnapshot {
            identity,
            value: ProcessNetworkSnapshot {
                state: DeviceState::healthy(10),
                traffic_state: DeviceState::healthy(10),
                ..ProcessNetworkSnapshot::default()
            },
        },
    }));
    assert!(matches!(
        projection.apply(&network),
        ProcessInsightsProjectionApplyResult::AppliedPartial(_)
    ));
    assert!(matches!(
        projection.apply_failure(
            &selected,
            revision,
            ProcessInsightFacet::Gpu,
            ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
        ),
        ProcessInsightsProjectionApplyResult::AppliedPartial(_)
    ));
    let resources = ProcessInsightFacetEvent::Resources(Box::new(ProcessInsightObservation {
        target: selected.clone(),
        revision,
        snapshot: ProcessInsightSnapshot {
            identity,
            value: ProcessResourceSnapshot::from_observations(
                DeviceState::healthy(10),
                ProcessResourceObservations::default(),
                Vec::new(),
            ),
        },
    }));
    assert!(matches!(
        projection.apply(&resources),
        ProcessInsightsProjectionApplyResult::AppliedPartial(_)
    ));
    let isolation = ProcessInsightFacetEvent::Isolation(Box::new(ProcessInsightObservation {
        target: selected,
        revision,
        snapshot: ProcessInsightSnapshot {
            identity,
            value: ProcessIsolation {
                state: DeviceState::healthy(10),
                ..ProcessIsolation::default()
            },
        },
    }));
    let ProcessInsightsProjectionApplyResult::AppliedComplete {
        projection,
        complete_snapshot,
    } = projection.apply(&isolation)
    else {
        panic!("last terminal facet must produce a compatibility snapshot");
    };

    assert!(matches!(
        projection.gpu,
        ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
            FailureKind::PermissionDenied
        ))
    ));
    assert_eq!(complete_snapshot.state, DeviceState::healthy(10));
    assert_eq!(
        complete_snapshot.gpu.state,
        DeviceState {
            status: DeviceStatus::PermissionDenied,
            last_success_ms: None,
        }
    );
    assert_eq!(complete_snapshot.network.state, DeviceState::healthy(10));
    assert_eq!(
        complete_snapshot.resources.state(),
        DeviceState::healthy(10)
    );
    assert_eq!(complete_snapshot.isolation.state, DeviceState::healthy(10));
}

#[test]
fn all_unavailable_terminal_projection_does_not_fabricate_identity_or_ready_snapshot() {
    let selected = target(100);
    let revision = ProcessInsightsRevision::new(1);
    let mut projection = ProcessInsightsProjection::default();
    projection.begin(selected.clone(), revision);
    for facet in [
        ProcessInsightFacet::Network,
        ProcessInsightFacet::Gpu,
        ProcessInsightFacet::Resources,
        ProcessInsightFacet::Isolation,
    ] {
        assert!(matches!(
            projection.apply_failure(
                &selected,
                revision,
                facet,
                ProcessInsightUnavailable::Provider(FailureKind::PermissionDenied),
            ),
            ProcessInsightsProjectionApplyResult::AppliedPartial(_)
        ));
    }
    let current = projection
        .current()
        .expect("terminal projection remains typed");
    assert_eq!(current.raw_identity(), None);
    assert_eq!(current.complete_snapshot(), None);
}
