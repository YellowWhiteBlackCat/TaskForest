use std::collections::BTreeMap;

use taskmanager_core::{
    CpuMetrics, CpuTelemetryObservation, DeviceId, DeviceLifecycle, DevicePresence, DeviceState,
    FailureKind, GpuTelemetryObservation, HostRuntimeFacts, HostRuntimeObservation, MemoryMetrics,
    MemoryTelemetryObservation, NetworkTelemetryObservation, ScalarObservation,
    StorageTelemetryObservation,
};
use taskmanager_platform_contract::{
    CapabilityId, EventSequence, ProviderId, RequestId, SourceOutcome, SourceStatus,
};

use crate::device_lifecycle::{DeviceLifecyclePartition, DeviceLifecycleProjection};
use crate::platform::SystemTelemetryDomainOutcome;
use crate::platform::event_batch::{
    CorrelatedEvent, CorrelatedSystemTelemetryOutcome, PlatformEventContext,
};

use super::*;

fn source(name: &'static str) -> Vec<SourceStatus> {
    vec![SourceStatus {
        provider: ProviderId::borrowed(name),
        outcome: SourceOutcome::Available,
        item_count: 1,
    }]
}

fn host(revision: SystemTelemetryRevision) -> SystemTelemetryDomainEvent {
    SystemTelemetryDomainEvent::Host {
        revision,
        observation: Box::new(HostRuntimeObservation::current(
            HostRuntimeFacts {
                uptime_secs: ScalarObservation::available(100, 10),
                processes: ScalarObservation::available(5, 10),
                threads: ScalarObservation::available(9, 10),
            },
            10,
            source("fixture.host"),
        )),
    }
}

fn cpu(revision: SystemTelemetryRevision) -> SystemTelemetryDomainEvent {
    SystemTelemetryDomainEvent::Cpu {
        revision,
        observation: Box::new(CpuTelemetryObservation::current(
            CpuMetrics::default(),
            11,
            source("fixture.cpu"),
        )),
    }
}

fn memory(revision: SystemTelemetryRevision) -> SystemTelemetryDomainEvent {
    SystemTelemetryDomainEvent::Memory {
        revision,
        observation: Box::new(MemoryTelemetryObservation::current(
            MemoryMetrics::default(),
            12,
            source("fixture.memory"),
        )),
    }
}

fn storage(revision: SystemTelemetryRevision) -> SystemTelemetryDomainEvent {
    SystemTelemetryDomainEvent::Storage {
        revision,
        observation: Box::new(StorageTelemetryObservation::current(
            Vec::new(),
            13,
            source("fixture.storage"),
            Vec::new(),
            BTreeMap::new(),
        )),
    }
}

fn network(revision: SystemTelemetryRevision) -> SystemTelemetryDomainEvent {
    SystemTelemetryDomainEvent::Network {
        revision,
        observation: Box::new(NetworkTelemetryObservation::current(
            Vec::new(),
            14,
            source("fixture.network"),
            Vec::new(),
            BTreeMap::new(),
        )),
    }
}

fn gpu(revision: SystemTelemetryRevision) -> SystemTelemetryDomainEvent {
    SystemTelemetryDomainEvent::Gpu {
        revision,
        observation: Box::new(GpuTelemetryObservation::current(
            Vec::new(),
            15,
            source("fixture.gpu"),
            Vec::new(),
            BTreeMap::new(),
        )),
    }
}

#[test]
fn each_domain_applies_immediately_and_complete_current_set_yields_snapshot() {
    let revision = SystemTelemetryRevision::new(1);
    let mut projection = SystemTelemetryProjection::default();
    projection.begin(revision);
    let events = [
        host(revision),
        cpu(revision),
        memory(revision),
        storage(revision),
        network(revision),
        gpu(revision),
    ];

    for event in &events[..5] {
        assert!(matches!(
            projection.apply(event),
            SystemTelemetryProjectionApplyResult::AppliedPartial(_)
        ));
    }
    let terminal = projection.apply(&events[5]);
    let SystemTelemetryProjectionApplyResult::AppliedTerminal { projection } = terminal else {
        panic!("six current domains must produce a complete snapshot");
    };
    let snapshot = projection
        .complete_snapshot()
        .expect("current terminal projection has a complete snapshot");
    assert!(projection.is_terminal());
    assert_eq!(snapshot.timestamp_ms, 15);
    assert_eq!(snapshot.uptime_secs, 100);
    assert_eq!(snapshot.processes, 5);
    assert_eq!(snapshot.threads, Some(9));
}

#[test]
fn all_failed_domains_never_fabricate_a_complete_snapshot() {
    let revision = SystemTelemetryRevision::new(2);
    let mut projection = SystemTelemetryProjection::default();
    projection.begin(revision);
    let domains = [
        SystemTelemetryDomain::Host,
        SystemTelemetryDomain::Cpu,
        SystemTelemetryDomain::Memory,
        SystemTelemetryDomain::Storage,
        SystemTelemetryDomain::Network,
        SystemTelemetryDomain::Gpu,
    ];
    let mut result = None;
    for domain in domains {
        result = Some(projection.apply_failure(
            revision,
            domain,
            SystemTelemetryUnavailable::Provider(FailureKind::PermissionDenied),
        ));
    }
    let Some(SystemTelemetryProjectionApplyResult::AppliedTerminal { projection }) = result else {
        panic!("all domains resolved to a terminal projection");
    };
    assert!(projection.complete_snapshot().is_none());
}

#[test]
fn stale_observation_is_not_promoted_to_current_complete_data() {
    let revision = SystemTelemetryRevision::new(3);
    let mut projection = SystemTelemetryProjection::default();
    projection.begin(revision);
    for event in [
        host(revision),
        cpu(revision),
        memory(revision),
        storage(revision),
        network(revision),
    ] {
        let _ = projection.apply(&event);
    }
    let stale_gpu = SystemTelemetryDomainEvent::Gpu {
        revision,
        observation: Box::new(GpuTelemetryObservation::stale(
            Vec::new(),
            9,
            FailureKind::TimedOut,
            source("fixture.gpu"),
            Vec::new(),
            BTreeMap::new(),
        )),
    };

    let SystemTelemetryProjectionApplyResult::AppliedTerminal {
        projection: terminal,
    } = projection.apply(&stale_gpu)
    else {
        panic!("stale GPU still resolves the projection");
    };
    assert!(terminal.complete_snapshot().is_none());
    assert!(matches!(
        projection.current().map(|current| &current.gpu),
        Some(SystemTelemetryDomainState::Stale(_))
    ));
}

#[test]
fn old_revision_and_duplicate_domain_are_rejected() {
    let revision = SystemTelemetryRevision::new(4);
    let mut projection = SystemTelemetryProjection::default();
    projection.begin(revision);

    assert!(matches!(
        projection.apply(&cpu(SystemTelemetryRevision::new(3))),
        SystemTelemetryProjectionApplyResult::Ignored(
            SystemTelemetryProjectionRejection::StaleOrUnexpectedRevision
        )
    ));
    assert!(matches!(
        projection.apply(&cpu(revision)),
        SystemTelemetryProjectionApplyResult::AppliedPartial(_)
    ));
    assert!(matches!(
        projection.apply(&cpu(revision)),
        SystemTelemetryProjectionApplyResult::Ignored(
            SystemTelemetryProjectionRejection::DuplicateDomain
        )
    ));
}

#[test]
fn newer_subset_refresh_reopens_only_due_domains_before_accepting_results() {
    let initial_revision = SystemTelemetryRevision::new(40);
    let refresh_revision = SystemTelemetryRevision::new(41);
    let mut projection = SystemTelemetryProjection::default();
    projection.begin(initial_revision);
    for event in [
        host(initial_revision),
        cpu(initial_revision),
        memory(initial_revision),
        storage(initial_revision),
        network(initial_revision),
        gpu(initial_revision),
    ] {
        let _ = projection.apply(&event);
    }

    projection.begin_domains(
        refresh_revision,
        &[SystemTelemetryDomain::Cpu, SystemTelemetryDomain::Gpu],
    );

    let reopened = projection.current().expect("new revision is active");
    assert_eq!(reopened.revision, refresh_revision);
    assert!(matches!(
        reopened.host,
        SystemTelemetryDomainState::Current(_)
    ));
    assert!(matches!(reopened.cpu, SystemTelemetryDomainState::Pending));
    assert!(matches!(reopened.gpu, SystemTelemetryDomainState::Pending));
    assert!(!reopened.is_terminal());
    assert!(matches!(
        projection.apply(&cpu(refresh_revision)),
        SystemTelemetryProjectionApplyResult::AppliedPartial(_)
    ));
    assert!(matches!(
        projection.apply(&gpu(refresh_revision)),
        SystemTelemetryProjectionApplyResult::AppliedTerminal { .. }
    ));
}

#[test]
fn partial_current_domain_remains_explicit_in_projection_state() {
    let revision = SystemTelemetryRevision::new(6);
    let event = SystemTelemetryDomainEvent::Cpu {
        revision,
        observation: Box::new(CpuTelemetryObservation::partial(
            CpuMetrics::default(),
            10,
            FailureKind::TimedOut,
            source("fixture.cpu"),
        )),
    };
    let mut projection = SystemTelemetryProjection::default();
    projection.begin(revision);

    let _ = projection.apply(&event);

    assert!(matches!(
        projection.current().map(|current| &current.cpu),
        Some(SystemTelemetryDomainState::Partial(_))
    ));
}

#[test]
fn equal_lifecycle_with_same_id_across_domains_fails_closed() {
    let revision = SystemTelemetryRevision::new(5);
    let lifecycle = DeviceLifecycle {
        presence: DevicePresence::Present,
        state: DeviceState::healthy(10),
        generation: 1,
        first_seen_ms: Some(10),
        last_seen_ms: Some(10),
        absent_since_ms: None,
    };
    let id = DeviceId::new("shared-device-id");
    let storage = SystemTelemetryDomainEvent::Storage {
        revision,
        observation: Box::new(StorageTelemetryObservation::current(
            Vec::new(),
            10,
            source("fixture.storage"),
            Vec::new(),
            BTreeMap::from([(id.clone(), lifecycle)]),
        )),
    };
    let network = SystemTelemetryDomainEvent::Network {
        revision,
        observation: Box::new(NetworkTelemetryObservation::current(
            Vec::new(),
            11,
            source("fixture.network"),
            Vec::new(),
            BTreeMap::from([(id, lifecycle)]),
        )),
    };
    let mut projection = SystemTelemetryProjection::default();
    projection.begin(revision);

    let _ = projection.apply(&storage);
    assert!(matches!(
        projection.apply(&network),
        SystemTelemetryProjectionApplyResult::Ignored(
            SystemTelemetryProjectionRejection::ConflictingDeviceLifecycle
        )
    ));
    assert!(matches!(
        projection.current().map(|current| &current.network),
        Some(SystemTelemetryDomainState::Pending)
    ));
}

// ── frontend-shared acceptance policy ────────────────────────────────────────
// The GUI and TUI both apply this single policy through
// `ProjectedSystemTelemetry::accept_projection`; these tests pin it so the
// frontends can never drift.

/// Reduce `events` into the latest projection for the given revision.
fn projection_from(
    revision: SystemTelemetryRevision,
    events: impl IntoIterator<Item = SystemTelemetryDomainEvent>,
) -> ProjectedSystemTelemetry {
    let mut reducer = SystemTelemetryProjection::default();
    reducer.begin(revision);
    let mut latest = None;
    for event in events {
        latest = match reducer.apply(&event) {
            SystemTelemetryProjectionApplyResult::AppliedPartial(projection)
            | SystemTelemetryProjectionApplyResult::AppliedTerminal { projection } => {
                Some(*projection)
            }
            SystemTelemetryProjectionApplyResult::Ignored(reason) => {
                panic!("fixture projection rejected: {reason:?}")
            }
        };
    }
    latest.expect("fixture should contain an event")
}

fn full_projection(revision_value: u64, _observed_at_ms: u64) -> ProjectedSystemTelemetry {
    let revision = SystemTelemetryRevision::new(revision_value);
    projection_from(
        revision,
        [
            host(revision),
            cpu(revision),
            memory(revision),
            storage(revision),
            network(revision),
            gpu(revision),
        ],
    )
}

/// A projection whose GPU domain is still pending (only five domains applied).
fn incomplete_projection(revision_value: u64) -> ProjectedSystemTelemetry {
    let revision = SystemTelemetryRevision::new(revision_value);
    projection_from(
        revision,
        [
            host(revision),
            cpu(revision),
            memory(revision),
            storage(revision),
            network(revision),
        ],
    )
}

#[test]
fn acceptance_rejects_older_revisions_and_pending_regressions() {
    let mut latest = None;
    assert!(matches!(
        ProjectedSystemTelemetry::accept_projection(&mut latest, full_projection(5, 50)),
        ProjectionAcceptance::Accepted { snapshot: Some(_) }
    ));

    // Older revision: rejected, latest untouched.
    assert!(matches!(
        ProjectedSystemTelemetry::accept_projection(&mut latest, full_projection(4, 40)),
        ProjectionAcceptance::Rejected
    ));
    assert_eq!(
        latest.as_ref().map(|current| current.revision),
        Some(SystemTelemetryRevision::new(5))
    );

    // Same revision with a resolved domain regressing to Pending: rejected.
    assert!(matches!(
        ProjectedSystemTelemetry::accept_projection(&mut latest, incomplete_projection(5)),
        ProjectionAcceptance::Rejected
    ));
    assert!(matches!(
        latest.as_ref().map(|current| &current.gpu),
        Some(SystemTelemetryDomainState::Current(_))
    ));
}

#[test]
fn incomplete_or_failed_projections_stay_typed_without_a_complete_snapshot() {
    let mut latest = None;
    // A same-revision extension that is still incomplete: accepted as latest
    // typed state, but no render snapshot is fabricated.
    assert!(matches!(
        ProjectedSystemTelemetry::accept_projection(&mut latest, incomplete_projection(6)),
        ProjectionAcceptance::Accepted { snapshot: None }
    ));
    assert_eq!(
        latest.as_ref().map(|current| current.revision),
        Some(SystemTelemetryRevision::new(6))
    );

    // A terminal projection whose domains all failed likewise carries no
    // snapshot but remains the typed latest state.
    let revision = SystemTelemetryRevision::new(7);
    let mut reducer = SystemTelemetryProjection::default();
    reducer.begin(revision);
    let mut result = None;
    for domain in [
        SystemTelemetryDomain::Host,
        SystemTelemetryDomain::Cpu,
        SystemTelemetryDomain::Memory,
        SystemTelemetryDomain::Storage,
        SystemTelemetryDomain::Network,
        SystemTelemetryDomain::Gpu,
    ] {
        result = Some(reducer.apply_failure(
            revision,
            domain,
            SystemTelemetryUnavailable::Provider(FailureKind::PermissionDenied),
        ));
    }
    let SystemTelemetryProjectionApplyResult::AppliedTerminal {
        projection: unavailable,
    } = result.expect("all domains resolved")
    else {
        panic!("fixture should be terminal");
    };
    assert!(matches!(
        ProjectedSystemTelemetry::accept_projection(&mut latest, *unavailable),
        ProjectionAcceptance::Accepted { snapshot: None }
    ));
    assert!(matches!(
        latest.as_ref().map(|current| &current.gpu),
        Some(SystemTelemetryDomainState::Unavailable { .. })
    ));
}

#[test]
fn optional_host_threads_and_gpu_failure_do_not_blank_render_snapshot() {
    let revision = SystemTelemetryRevision::new(8);
    let host_without_threads = SystemTelemetryDomainEvent::Host {
        revision,
        observation: Box::new(HostRuntimeObservation::current(
            HostRuntimeFacts {
                uptime_secs: ScalarObservation::available(100, 10),
                processes: ScalarObservation::available(5, 10),
                threads: ScalarObservation::unavailable(FailureKind::Unsupported),
            },
            10,
            source("fixture.host"),
        )),
    };
    let mut reducer = SystemTelemetryProjection::default();
    reducer.begin(revision);
    for event in [
        host_without_threads,
        cpu(revision),
        memory(revision),
        storage(revision),
        network(revision),
    ] {
        assert!(matches!(
            reducer.apply(&event),
            SystemTelemetryProjectionApplyResult::AppliedPartial(_)
        ));
    }
    let _ = reducer.apply_failure(
        revision,
        SystemTelemetryDomain::Gpu,
        SystemTelemetryUnavailable::Provider(FailureKind::MissingDependency),
    );
    let incoming = reducer
        .snapshot()
        .expect("projection has all six domains resolved");
    let mut latest = None;
    let ProjectionAcceptance::Accepted {
        snapshot: Some(snapshot),
    } = ProjectedSystemTelemetry::accept_projection(&mut latest, incoming)
    else {
        panic!("optional facets must not block the render snapshot");
    };
    assert_eq!(snapshot.threads, None);
    assert!(snapshot.gpu.is_empty());
    assert!(snapshot.telemetry_sources.iter().any(|status| {
        status.provider.as_str() == "application.system.gpu"
            && status.outcome == SourceOutcome::Unavailable(FailureKind::MissingDependency)
    }));
}

fn correlated_outcome(
    sequence: u64,
    capability: CapabilityId,
    event: SystemTelemetryDomainEvent,
) -> CorrelatedSystemTelemetryOutcome {
    CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(sequence).expect("non-zero request"),
            capability,
            provider: None,
            sequence: EventSequence::new(sequence),
            observed_at_ms: sequence.saturating_mul(10),
        },
        SystemTelemetryDomainOutcome::Observed(event),
    )
}

#[test]
fn shared_lifecycle_policy_updates_three_independent_partitions() {
    let revision = SystemTelemetryRevision::new(1);
    let lifecycle = DeviceLifecycle {
        presence: DevicePresence::Present,
        state: DeviceState::healthy(10),
        generation: 1,
        first_seen_ms: Some(10),
        last_seen_ms: Some(10),
        absent_since_ms: None,
    };
    let storage = SystemTelemetryDomainEvent::Storage {
        revision,
        observation: Box::new(StorageTelemetryObservation::current(
            Vec::new(),
            10,
            source("fixture.storage"),
            Vec::new(),
            BTreeMap::from([(DeviceId::new("fixture:disk"), lifecycle)]),
        )),
    };
    let network = SystemTelemetryDomainEvent::Network {
        revision,
        observation: Box::new(NetworkTelemetryObservation::current(
            Vec::new(),
            20,
            source("fixture.network"),
            Vec::new(),
            BTreeMap::from([(DeviceId::new("fixture:network"), lifecycle)]),
        )),
    };
    let gpu = SystemTelemetryDomainEvent::Gpu {
        revision,
        observation: Box::new(GpuTelemetryObservation::current(
            Vec::new(),
            30,
            source("fixture.gpu"),
            Vec::new(),
            BTreeMap::from([(DeviceId::new("fixture:gpu"), lifecycle)]),
        )),
    };
    let mut projection = DeviceLifecycleProjection::default();
    let mut diagnostics = DeviceLifecycleDiagnosticHistory::default();

    for (sequence, capability, event) in [
        (1, CapabilityId::TELEMETRY_STORAGE, storage),
        (2, CapabilityId::TELEMETRY_NETWORK, network),
        (3, CapabilityId::TELEMETRY_GPU, gpu),
    ] {
        apply_system_outcome_lifecycle(
            &mut projection,
            &mut diagnostics,
            &correlated_outcome(sequence, capability, event),
        );
    }

    assert_eq!(
        projection.authority("fixture:disk"),
        Some(DeviceLifecyclePartition::SystemStorage)
    );
    assert_eq!(
        projection.authority("fixture:network"),
        Some(DeviceLifecyclePartition::SystemNetwork)
    );
    assert_eq!(
        projection.authority("fixture:gpu"),
        Some(DeviceLifecyclePartition::SystemGpu)
    );
    assert_eq!(diagnostics.len(), 3);

    // CPU / memory / host outcomes carry no sidecar: lifecycle untouched.
    apply_system_outcome_lifecycle(
        &mut projection,
        &mut diagnostics,
        &correlated_outcome(4, CapabilityId::TELEMETRY_CPU, cpu(revision)),
    );
    assert_eq!(diagnostics.len(), 3);
}
