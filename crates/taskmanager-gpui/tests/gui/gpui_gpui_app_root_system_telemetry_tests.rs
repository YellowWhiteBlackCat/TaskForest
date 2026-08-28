use std::collections::BTreeMap;

use std::rc::Rc;

use taskmanager_application::{
    CapabilityId, CorrelatedEvent, DeviceLifecycleDiagnosticHistory, DeviceLifecyclePartition,
    DeviceLifecycleProjection, EventSequence, PlatformEventContext, ProjectedSystemTelemetry,
    ProjectionAcceptance, RequestId, SystemTelemetryDomainEvent, SystemTelemetryDomainOutcome,
    SystemTelemetryProjection, SystemTelemetryProjectionApplyResult, SystemTelemetryRevision,
    SystemTelemetryUnavailable,
};
use taskmanager_telemetry_store::{CorrelatedIngestionError, SystemHistoryDomain, TelemetryStore};

use crate::core::{
    CpuMetrics, CpuScalarObservations, CpuTelemetryObservation, DeviceId, DeviceLifecycle,
    DevicePresence, DeviceState, FailureKind, GpuTelemetryObservation, HostRuntimeFacts,
    HostRuntimeObservation, MemoryMetrics, MemoryTelemetryObservation, NetworkTelemetryObservation,
    ScalarObservation, StorageTelemetryObservation, SystemSnapshot,
};

use super::*;

fn measured_cpu(value: f32, observed_at_ms: u64) -> CpuMetrics {
    CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(value, observed_at_ms),
        ..Default::default()
    })
}

/// Accept a monotonic projection and update the cache when the required
/// domains contain current facts. Optional host/GPU facets never erase the
/// independently readable CPU, memory, storage, or network values.
///
/// The monotonicity policy and the typed latest-state store live in
/// `taskmanager-application` (`ProjectedSystemTelemetry::accept_projection`,
/// shared with the TUI); this wrapper only maps the shared acceptance onto
/// the GUI's concrete render cache (`SystemSnapshot`).
///
/// Returns `true` only when `cached` changed. Pending, stale, unavailable, or
/// incomplete required projections still become the latest typed UI state but
/// never fabricate default values or publish retained data as current.
fn apply_projected_system_telemetry(
    latest: &mut Option<ProjectedSystemTelemetry>,
    cached: &mut Rc<SystemSnapshot>,
    incoming: ProjectedSystemTelemetry,
) -> bool {
    match ProjectedSystemTelemetry::accept_projection(latest, incoming) {
        ProjectionAcceptance::Rejected => false,
        ProjectionAcceptance::Accepted {
            snapshot: Some(snapshot),
        } => {
            *cached = Rc::new(*snapshot);
            true
        }
        ProjectionAcceptance::Accepted { snapshot: None } => false,
    }
}

/// Apply lifecycle sidecars directly from accepted storage/network/GPU
/// observations. CPU, memory, and host events intentionally have no lifecycle
/// effect, and no aggregate `SystemSnapshot` is synthesized. The shared policy
/// lives in `taskmanager-application::apply_system_outcome_lifecycle`.
fn apply_system_domain_lifecycle(
    projection: &mut DeviceLifecycleProjection,
    diagnostics: &mut DeviceLifecycleDiagnosticHistory,
    correlated: &CorrelatedSystemTelemetryOutcome,
) {
    taskmanager_application::apply_system_outcome_lifecycle(projection, diagnostics, correlated);
}

fn host(observed_at_ms: u64) -> HostRuntimeObservation {
    HostRuntimeObservation::current(
        HostRuntimeFacts {
            uptime_secs: ScalarObservation::available(100, observed_at_ms),
            processes: ScalarObservation::available(4, observed_at_ms),
            threads: ScalarObservation::available(8, observed_at_ms),
        },
        observed_at_ms,
        Vec::new(),
    )
}

fn events(
    revision: SystemTelemetryRevision,
    observed_at_ms: u64,
    host: HostRuntimeObservation,
    cpu: CpuTelemetryObservation,
) -> Vec<SystemTelemetryDomainEvent> {
    vec![
        SystemTelemetryDomainEvent::Host {
            revision,
            observation: Box::new(host),
        },
        SystemTelemetryDomainEvent::Cpu {
            revision,
            observation: Box::new(cpu),
        },
        SystemTelemetryDomainEvent::Memory {
            revision,
            observation: Box::new(MemoryTelemetryObservation::current(
                MemoryMetrics::default(),
                observed_at_ms,
                Vec::new(),
            )),
        },
        SystemTelemetryDomainEvent::Storage {
            revision,
            observation: Box::new(StorageTelemetryObservation::current(
                Vec::new(),
                observed_at_ms,
                Vec::new(),
                Vec::new(),
                BTreeMap::new(),
            )),
        },
        SystemTelemetryDomainEvent::Network {
            revision,
            observation: Box::new(NetworkTelemetryObservation::current(
                Vec::new(),
                observed_at_ms,
                Vec::new(),
                Vec::new(),
                BTreeMap::new(),
            )),
        },
        SystemTelemetryDomainEvent::Gpu {
            revision,
            observation: Box::new(GpuTelemetryObservation::current(
                Vec::new(),
                observed_at_ms,
                Vec::new(),
                Vec::new(),
                BTreeMap::new(),
            )),
        },
    ]
}

fn projection_from(
    revision: SystemTelemetryRevision,
    events: Vec<SystemTelemetryDomainEvent>,
) -> ProjectedSystemTelemetry {
    let mut projection = SystemTelemetryProjection::default();
    projection.begin(revision);
    let mut latest = None;
    for event in events {
        latest = match projection.apply(&event) {
            SystemTelemetryProjectionApplyResult::AppliedPartial(value) => Some(*value),
            SystemTelemetryProjectionApplyResult::AppliedTerminal { projection } => {
                Some(*projection)
            }
            SystemTelemetryProjectionApplyResult::Ignored(reason) => {
                panic!("fixture projection was ignored: {reason:?}")
            }
        };
    }
    latest.expect("fixture should apply at least one event")
}

fn full_projection(revision_value: u64, observed_at_ms: u64) -> ProjectedSystemTelemetry {
    let revision = SystemTelemetryRevision::new(revision_value);
    projection_from(
        revision,
        events(
            revision,
            observed_at_ms,
            host(observed_at_ms),
            CpuTelemetryObservation::current(CpuMetrics::default(), observed_at_ms, Vec::new()),
        ),
    )
}

fn unavailable_gpu_projection(
    revision: SystemTelemetryRevision,
    observed_at_ms: u64,
) -> ProjectedSystemTelemetry {
    let mut projection = SystemTelemetryProjection::default();
    projection.begin(revision);
    for event in events(
        revision,
        observed_at_ms,
        host(observed_at_ms),
        CpuTelemetryObservation::current(CpuMetrics::default(), observed_at_ms, Vec::new()),
    )
    .into_iter()
    .take(5)
    {
        assert!(matches!(
            projection.apply(&event),
            SystemTelemetryProjectionApplyResult::AppliedPartial(_)
        ));
    }
    match projection.apply_failure(
        revision,
        SystemTelemetryDomain::Gpu,
        SystemTelemetryUnavailable::Provider(FailureKind::PermissionDenied),
    ) {
        SystemTelemetryProjectionApplyResult::AppliedTerminal { projection } => *projection,
        result => panic!("fixture should be terminal: {result:?}"),
    }
}

#[test]
fn partial_projection_becomes_latest_without_replacing_cached_snapshot() {
    let revision = SystemTelemetryRevision::new(2);
    let partial = projection_from(
        revision,
        vec![SystemTelemetryDomainEvent::Cpu {
            revision,
            observation: Box::new(CpuTelemetryObservation::current(
                CpuMetrics::default(),
                20,
                Vec::new(),
            )),
        }],
    );
    let mut latest = None;
    let mut cached = Rc::new(SystemSnapshot {
        timestamp_ms: 10,
        ..SystemSnapshot::default()
    });

    assert!(!apply_projected_system_telemetry(
        &mut latest,
        &mut cached,
        partial,
    ));
    assert_eq!(cached.timestamp_ms, 10);
    assert_eq!(latest.as_ref().map(|value| value.revision), Some(revision));
}

#[test]
fn stale_domain_and_incomplete_host_scalars_never_replace_cache() {
    let revision = SystemTelemetryRevision::new(3);
    let stale = projection_from(
        revision,
        events(
            revision,
            30,
            host(30),
            CpuTelemetryObservation::stale(
                CpuMetrics::default(),
                29,
                FailureKind::TemporarilyUnavailable,
                Vec::new(),
            ),
        ),
    );
    let incomplete_revision = SystemTelemetryRevision::new(4);
    let incomplete_host = HostRuntimeObservation::partial(
        HostRuntimeFacts {
            uptime_secs: ScalarObservation::available(100, 40),
            processes: ScalarObservation::unavailable(FailureKind::PermissionDenied),
            threads: ScalarObservation::available(8, 40),
        },
        40,
        FailureKind::PermissionDenied,
        Vec::new(),
    );
    let incomplete = projection_from(
        incomplete_revision,
        events(
            incomplete_revision,
            40,
            incomplete_host,
            CpuTelemetryObservation::current(CpuMetrics::default(), 40, Vec::new()),
        ),
    );
    let mut latest = None;
    let mut cached = Rc::new(SystemSnapshot {
        timestamp_ms: 11,
        ..SystemSnapshot::default()
    });

    assert!(!apply_projected_system_telemetry(
        &mut latest,
        &mut cached,
        stale,
    ));
    assert!(!apply_projected_system_telemetry(
        &mut latest,
        &mut cached,
        incomplete,
    ));
    assert_eq!(cached.timestamp_ms, 11);
    assert_eq!(
        latest.as_ref().map(|value| value.revision),
        Some(incomplete_revision)
    );
}

#[test]
fn unavailable_terminal_projection_remains_typed_without_defaulting_cache() {
    let revision = SystemTelemetryRevision::new(6);
    let mut latest = None;
    let mut cached = Rc::new(SystemSnapshot {
        timestamp_ms: 55,
        ..SystemSnapshot::default()
    });

    assert!(apply_projected_system_telemetry(
        &mut latest,
        &mut cached,
        unavailable_gpu_projection(revision, 60),
    ));
    assert_eq!(cached.timestamp_ms, 60);
    assert_eq!(latest.as_ref().map(|value| value.revision), Some(revision));
}

#[test]
fn only_complete_current_projection_replaces_cache_and_older_revision_is_ignored() {
    let mut latest = None;
    let mut cached = Rc::new(SystemSnapshot::default());

    assert!(apply_projected_system_telemetry(
        &mut latest,
        &mut cached,
        full_projection(5, 50),
    ));
    assert_eq!(cached.timestamp_ms, 50);
    assert!(!apply_projected_system_telemetry(
        &mut latest,
        &mut cached,
        full_projection(4, 40),
    ));
    assert_eq!(cached.timestamp_ms, 50);
    assert_eq!(
        latest.as_ref().map(|value| value.revision),
        Some(SystemTelemetryRevision::new(5))
    );
}

fn correlated(
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

fn lifecycle(now_ms: u64) -> DeviceLifecycle {
    DeviceLifecycle {
        presence: DevicePresence::Present,
        state: DeviceState::healthy(now_ms),
        generation: 1,
        first_seen_ms: Some(now_ms),
        last_seen_ms: Some(now_ms),
        absent_since_ms: None,
    }
}

#[test]
fn mc02_partition_recovery_case_device_lifecycles_apply_as_three_independent_typed_partitions() {
    let revision = SystemTelemetryRevision::new(1);
    let storage = correlated(
        1,
        CapabilityId::TELEMETRY_STORAGE,
        SystemTelemetryDomainEvent::Storage {
            revision,
            observation: Box::new(StorageTelemetryObservation::current(
                Vec::new(),
                10,
                Vec::new(),
                Vec::new(),
                BTreeMap::from([(DeviceId::new("fixture:disk"), lifecycle(10))]),
            )),
        },
    );
    let network = correlated(
        2,
        CapabilityId::TELEMETRY_NETWORK,
        SystemTelemetryDomainEvent::Network {
            revision,
            observation: Box::new(NetworkTelemetryObservation::current(
                Vec::new(),
                10,
                Vec::new(),
                Vec::new(),
                BTreeMap::from([(DeviceId::new("fixture:net"), lifecycle(10))]),
            )),
        },
    );
    let gpu = correlated(
        3,
        CapabilityId::TELEMETRY_GPU,
        SystemTelemetryDomainEvent::Gpu {
            revision,
            observation: Box::new(GpuTelemetryObservation::current(
                Vec::new(),
                10,
                Vec::new(),
                Vec::new(),
                BTreeMap::from([(DeviceId::new("fixture:gpu"), lifecycle(10))]),
            )),
        },
    );
    let mut projection = DeviceLifecycleProjection::default();
    let mut diagnostics = DeviceLifecycleDiagnosticHistory::default();

    apply_system_domain_lifecycle(&mut projection, &mut diagnostics, &storage);
    apply_system_domain_lifecycle(&mut projection, &mut diagnostics, &network);
    apply_system_domain_lifecycle(&mut projection, &mut diagnostics, &gpu);

    assert_eq!(
        projection.authority("fixture:disk"),
        Some(DeviceLifecyclePartition::SystemStorage)
    );
    assert_eq!(
        projection.authority("fixture:net"),
        Some(DeviceLifecyclePartition::SystemNetwork)
    );
    assert_eq!(
        projection.authority("fixture:gpu"),
        Some(DeviceLifecyclePartition::SystemGpu)
    );
    assert_eq!(diagnostics.len(), 3);
}

#[test]
fn accepted_outcome_writes_measurement_and_completion_times() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(2);
    let revision = SystemTelemetryRevision::new(7);
    let event = correlated(
        9,
        CapabilityId::TELEMETRY_CPU,
        SystemTelemetryDomainEvent::Cpu {
            revision,
            observation: Box::new(CpuTelemetryObservation::current(
                CpuMetrics::default(),
                90,
                Vec::new(),
            )),
        },
    );

    ingest_correlated_system_outcome(&ingestor, &event)
        .expect("accepted outcome should enter history");
    let sample = &store.system_history.cpu_usage().samples()[0];
    assert_eq!(sample.stamp.revision(), 7);
    assert_eq!(sample.stamp.completed_at_ms(), 90);
    assert_eq!(sample.measured_at_ms, Some(90));
}

#[test]
fn accepted_failure_advances_history_with_an_explicit_gap() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(3);
    let observed = correlated(
        1,
        CapabilityId::TELEMETRY_CPU,
        SystemTelemetryDomainEvent::Cpu {
            revision: SystemTelemetryRevision::new(1),
            observation: Box::new(CpuTelemetryObservation::current(
                measured_cpu(12.0, 10),
                10,
                Vec::new(),
            )),
        },
    );
    let unavailable = CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::new(2).expect("non-zero request"),
            capability: CapabilityId::TELEMETRY_CPU,
            provider: None,
            sequence: EventSequence::new(2),
            observed_at_ms: 20,
        },
        SystemTelemetryDomainOutcome::Unavailable {
            revision: SystemTelemetryRevision::new(2),
            domain: SystemTelemetryDomain::Cpu,
            reason: SystemTelemetryUnavailable::Provider(FailureKind::TimedOut),
        },
    );

    ingest_correlated_system_outcome(&ingestor, &observed).expect("accepted observation");
    ingest_correlated_system_outcome(&ingestor, &unavailable).expect("accepted failure");

    assert_eq!(
        store
            .system_history
            .cpu_usage()
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(12.0), None]
    );
}

#[test]
fn stale_outcome_is_diagnosed_without_polluting_history() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(3);
    let current = correlated(
        2,
        CapabilityId::TELEMETRY_CPU,
        SystemTelemetryDomainEvent::Cpu {
            revision: SystemTelemetryRevision::new(2),
            observation: Box::new(CpuTelemetryObservation::current(
                measured_cpu(22.0, 20),
                20,
                Vec::new(),
            )),
        },
    );
    let stale = correlated(
        3,
        CapabilityId::TELEMETRY_CPU,
        SystemTelemetryDomainEvent::Cpu {
            revision: SystemTelemetryRevision::new(1),
            observation: Box::new(CpuTelemetryObservation::current(
                measured_cpu(99.0, 10),
                10,
                Vec::new(),
            )),
        },
    );

    ingest_correlated_system_outcome(&ingestor, &current).expect("current outcome");
    let error = ingest_correlated_system_outcome(&ingestor, &stale)
        .expect_err("stale revision must fail closed");
    let mut diagnostics = Vec::new();
    record_history_ingestion_error(&mut diagnostics, &stale, error);

    assert_eq!(store.system_history.cpu_usage().samples().len(), 1);
    assert_eq!(
        store.system_history.cpu_usage().samples()[0].value,
        Some(22.0)
    );
    assert_eq!(
        diagnostics,
        [SystemHistoryIngestionDiagnostic {
            revision: SystemTelemetryRevision::new(1),
            domain: SystemTelemetryDomain::Cpu,
            error: SystemHistoryIngestionError::Store(
                CorrelatedIngestionError::NonIncreasingRevision {
                    domain: SystemHistoryDomain::Cpu,
                    last_revision: 2,
                    rejected_revision: 1,
                }
            ),
        }]
    );
}

#[test]
fn zero_revision_and_completion_before_measurement_fail_closed_at_root_boundary() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(3);
    let zero = correlated(
        1,
        CapabilityId::TELEMETRY_CPU,
        SystemTelemetryDomainEvent::Cpu {
            revision: SystemTelemetryRevision::new(0),
            observation: Box::new(CpuTelemetryObservation::current(
                CpuMetrics::default(),
                10,
                Vec::new(),
            )),
        },
    );
    assert_eq!(
        ingest_correlated_system_outcome(&ingestor, &zero),
        Err(SystemHistoryIngestionError::InvalidZeroRevision)
    );

    let impossible = correlated(
        2,
        CapabilityId::TELEMETRY_CPU,
        SystemTelemetryDomainEvent::Cpu {
            revision: SystemTelemetryRevision::new(1),
            observation: Box::new(CpuTelemetryObservation::current(
                CpuMetrics::default(),
                21,
                Vec::new(),
            )),
        },
    );
    assert_eq!(
        ingest_correlated_system_outcome(&ingestor, &impossible),
        Err(SystemHistoryIngestionError::Store(
            CorrelatedIngestionError::CompletionPrecedesMeasurement {
                domain: SystemHistoryDomain::Cpu,
                measured_at_ms: 21,
                completed_at_ms: 20,
            }
        ))
    );
    assert!(store.system_history.cpu_usage().samples().is_empty());
}

#[test]
fn history_ingestion_diagnostics_are_bounded() {
    let template = correlated(
        1,
        CapabilityId::TELEMETRY_CPU,
        SystemTelemetryDomainEvent::Cpu {
            revision: SystemTelemetryRevision::new(1),
            observation: Box::new(CpuTelemetryObservation::default()),
        },
    );
    let mut diagnostics = Vec::new();
    for _ in 0..40 {
        record_history_ingestion_error(
            &mut diagnostics,
            &template,
            SystemHistoryIngestionError::InvalidZeroRevision,
        );
    }

    assert_eq!(diagnostics.len(), 32);
}
