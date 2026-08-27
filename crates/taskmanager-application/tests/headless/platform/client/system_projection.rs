use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityId, CapabilityRequest, CapabilityScheduler, CapabilitySnapshot,
    EventEnvelope, EventPort, EventPortError, EventSequence, FailureKind, OperationFailure,
    ProviderFailure, ProviderId, RequestEnvelope, RequestId, RequestPort,
    RuntimeSchedulingSnapshot, SubmissionError,
};

use super::*;
use crate::platform::{
    CorrelatedEvent, CpuTelemetryRequest, GpuTelemetryRequest, HostTelemetryRequest,
    MemoryTelemetryRequest, NetworkTelemetryRequest, PlatformEvent, PlatformFacets, PlatformHandle,
    StorageTelemetryRequest, SystemFacets, SystemTelemetryDomain, SystemTelemetryDomainEvent,
    SystemTelemetryDomainOutcome, SystemTelemetryRevision, SystemTelemetryUnavailable,
};
use crate::{CpuMetrics, CpuScalarObservations, CpuTelemetryObservation, ScalarObservation};

#[derive(Default)]
struct EmptyCapabilities;

impl CapabilityCatalog for EmptyCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::default()
    }
}

#[derive(Default)]
struct EmptyEvents;

impl EventPort for EmptyEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

struct DueScheduler(Mutex<Vec<CapabilityId>>);

impl CapabilityScheduler for DueScheduler {
    fn poll_due(&self, _observed_at_wall_ms: u64) -> Vec<CapabilityId> {
        std::mem::take(&mut *self.0.lock().expect("due scheduler"))
    }

    fn mark_submission_failed(&self, _capability: &CapabilityId, _failed_at_wall_ms: u64) {}

    fn set_cadence_ms(&self, _capability: &CapabilityId, _cadence_ms: Option<u64>) {}

    fn request_recovery(
        &self,
        _capability: &CapabilityId,
        _trigger: taskmanager_platform_contract::CapabilityRecoveryTrigger,
    ) -> taskmanager_platform_contract::CapabilityRecoveryOutcome {
        taskmanager_platform_contract::CapabilityRecoveryOutcome::UnknownCapability
    }

    fn scheduling_snapshot(&self) -> RuntimeSchedulingSnapshot {
        RuntimeSchedulingSnapshot::default()
    }
}

#[derive(Default)]
struct QueueEvents(Mutex<VecDeque<EventEnvelope<PlatformEvent>>>);

impl QueueEvents {
    fn push(&self, event: EventEnvelope<PlatformEvent>) {
        if let Ok(mut events) = self.0.lock() {
            events.push_back(event);
        }
    }
}

impl EventPort for QueueEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(self.0.lock().ok().and_then(|mut events| events.pop_front()))
    }
}

struct Accepting<T>(Mutex<Vec<T>>);

impl<T> Default for Accepting<T> {
    fn default() -> Self {
        Self(Mutex::new(Vec::new()))
    }
}

impl<T: CapabilityRequest> RequestPort for Accepting<T> {
    type Request = T;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        if let Ok(mut requests) = self.0.lock() {
            requests.push(request.payload);
        }
        Ok(())
    }
}

struct SystemFacetFixture {
    facets: SystemFacets,
    host: Arc<Accepting<HostTelemetryRequest>>,
    cpu: Arc<Accepting<CpuTelemetryRequest>>,
    memory: Arc<Accepting<MemoryTelemetryRequest>>,
    storage: Arc<Accepting<StorageTelemetryRequest>>,
    network: Arc<Accepting<NetworkTelemetryRequest>>,
    gpu: Arc<Accepting<GpuTelemetryRequest>>,
}

fn complete_system_facets() -> SystemFacetFixture {
    let host = Arc::new(Accepting::default());
    let cpu = Arc::new(Accepting::default());
    let memory = Arc::new(Accepting::default());
    let storage = Arc::new(Accepting::default());
    let network = Arc::new(Accepting::default());
    let gpu = Arc::new(Accepting::default());
    let facets = SystemFacets::default()
        .with_host(host.clone())
        .with_cpu(cpu.clone())
        .with_memory(memory.clone())
        .with_storage(storage.clone())
        .with_network(network.clone())
        .with_gpu(gpu.clone());
    SystemFacetFixture {
        facets,
        host,
        cpu,
        memory,
        storage,
        network,
        gpu,
    }
}

#[test]
fn production_refresh_submits_all_six_independent_domains() {
    let fixture = complete_system_facets();
    let facets = PlatformFacets::default().with_system(fixture.facets);
    let handle = PlatformHandle::new(Arc::new(EmptyCapabilities), Arc::new(EmptyEvents), facets);
    let mut client = PlatformClient::new(handle);

    let outcomes = client.request_refresh(crate::RefreshRequest::Telemetry, 100);

    assert_eq!(outcomes.len(), 6);
    assert!(outcomes.iter().all(Result::is_ok));
    assert_eq!(fixture.host.0.lock().expect("host recorder").len(), 1);
    assert_eq!(fixture.cpu.0.lock().expect("CPU recorder").len(), 1);
    assert_eq!(fixture.memory.0.lock().expect("memory recorder").len(), 1);
    assert_eq!(fixture.storage.0.lock().expect("storage recorder").len(), 1);
    assert_eq!(fixture.network.0.lock().expect("network recorder").len(), 1);
    assert_eq!(fixture.gpu.0.lock().expect("GPU recorder").len(), 1);
}

#[test]
fn scheduled_subset_submits_only_due_domains_under_one_revision() {
    let fixture = complete_system_facets();
    let scheduler = Arc::new(DueScheduler(Mutex::new(vec![
        CapabilityId::TELEMETRY_CPU,
        CapabilityId::TELEMETRY_GPU,
    ])));
    let facets = PlatformFacets::default().with_system(fixture.facets);
    let handle = PlatformHandle::new(Arc::new(EmptyCapabilities), Arc::new(EmptyEvents), facets)
        .with_scheduler(scheduler);
    let mut client = PlatformClient::new(handle);

    let outcomes = client.run_scheduled_refresh(100);

    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.iter().all(Result::is_ok));
    assert!(fixture.host.0.lock().expect("host recorder").is_empty());
    assert!(fixture.memory.0.lock().expect("memory recorder").is_empty());
    assert!(
        fixture
            .storage
            .0
            .lock()
            .expect("storage recorder")
            .is_empty()
    );
    assert!(
        fixture
            .network
            .0
            .lock()
            .expect("network recorder")
            .is_empty()
    );
    let cpu = fixture.cpu.0.lock().expect("CPU recorder");
    let gpu = fixture.gpu.0.lock().expect("GPU recorder");
    assert_eq!(cpu.len(), 1);
    assert_eq!(gpu.len(), 1);
    assert_eq!(
        cpu[0].revision, gpu[0].revision,
        "due siblings share one application correlation revision"
    );
}

#[test]
fn hung_old_refresh_tracking_remains_bounded_to_six_requests() {
    let fixture = complete_system_facets();
    let facets = PlatformFacets::default().with_system(fixture.facets);
    let handle = PlatformHandle::new(Arc::new(EmptyCapabilities), Arc::new(EmptyEvents), facets);
    let mut client = PlatformClient::new(handle);

    for submitted_at_ms in 1..=50 {
        let submission = client
            .submit_system_telemetry(submitted_at_ms)
            .expect("revision remains available");
        assert!(submission.has_pending_requests());
        assert_eq!(client.system_telemetry_requests.len(), 6);
    }
}

#[test]
fn revision_exhaustion_is_typed_and_submits_nothing() {
    let host = Arc::new(Accepting::<HostTelemetryRequest>::default());
    let facets =
        PlatformFacets::default().with_system(SystemFacets::default().with_host(host.clone()));
    let handle = PlatformHandle::new(Arc::new(EmptyCapabilities), Arc::new(EmptyEvents), facets);
    let mut client = PlatformClient::new(handle);
    client.system_telemetry_revision = SystemTelemetryRevision::new(u64::MAX);

    assert!(matches!(
        client.submit_system_telemetry(1),
        Err(crate::SystemTelemetrySubmissionError::RevisionExhausted)
    ));
    assert_eq!(host.0.lock().expect("host recorder").len(), 0);
    assert!(client.system_telemetry_requests.is_empty());

    let errors = client.request_refresh(crate::RefreshRequest::Telemetry, 2);
    assert_eq!(errors.len(), 6);
    assert_eq!(
        errors
            .into_iter()
            .map(|result| result.expect_err("exhausted revision").capability)
            .collect::<Vec<_>>(),
        [
            CapabilityId::TELEMETRY_HOST,
            CapabilityId::TELEMETRY_CPU,
            CapabilityId::TELEMETRY_MEMORY,
            CapabilityId::TELEMETRY_STORAGE,
            CapabilityId::TELEMETRY_NETWORK,
            CapabilityId::TELEMETRY_GPU,
        ]
    );
}

#[test]
fn stale_domain_event_becomes_a_typed_unavailable_outcome() {
    let cpu = Arc::new(Accepting::<CpuTelemetryRequest>::default());
    let events = Arc::new(QueueEvents::default());
    let facets = PlatformFacets::default().with_system(SystemFacets::default().with_cpu(cpu));
    let handle = PlatformHandle::new(Arc::new(EmptyCapabilities), events.clone(), facets);
    let mut client = PlatformClient::new(handle);
    let submission = client.submit_system_telemetry(1).expect("first revision");
    let request_id = submission.cpu.expect("CPU request accepted");
    events.push(EventEnvelope {
        request_id,
        capability: CapabilityId::TELEMETRY_CPU,
        provider: Some(ProviderId::borrowed("fixture.cpu")),
        sequence: EventSequence::new(1),
        observed_at_ms: 2,
        outcome: Ok(PlatformEvent::SystemTelemetry(
            SystemTelemetryDomainEvent::Cpu {
                revision: SystemTelemetryRevision::new(999),
                observation: Box::new(CpuTelemetryObservation::current(
                    CpuMetrics::default(),
                    2,
                    Vec::new(),
                )),
            },
        )),
    });

    let batch = client.try_drain().expect("event port remains live");

    assert_eq!(batch.failures.len(), 1);
    assert_eq!(
        batch.failures[0].kind,
        taskmanager_core::FailureKind::IdentityChanged
    );
    assert_eq!(batch.system_telemetry_projections.len(), 1);
    assert!(matches!(
        batch.system_telemetry_outcomes.as_slice(),
        [CorrelatedEvent {
            event: SystemTelemetryDomainOutcome::Unavailable {
                revision,
                domain: SystemTelemetryDomain::Cpu,
                reason: SystemTelemetryUnavailable::Provider(FailureKind::IdentityChanged),
            },
            ..
        }] if *revision == submission.revision
    ));
}

fn operation_failure(
    request_id: RequestId,
    capability: CapabilityId,
    sequence: EventSequence,
    kind: FailureKind,
    observed_at_ms: u64,
) -> OperationFailure {
    OperationFailure {
        request_id,
        capability,
        sequence,
        kind,
        retry: ProviderFailure::from_kind(kind).retry(),
        provider: Some(ProviderId::borrowed("fixture.cpu")),
        observed_at_ms,
    }
}

#[test]
fn accepted_runtime_failure_publishes_correlated_unavailable_outcome() {
    let cpu = Arc::new(Accepting::<CpuTelemetryRequest>::default());
    let events = Arc::new(QueueEvents::default());
    let facets = PlatformFacets::default().with_system(SystemFacets::default().with_cpu(cpu));
    let handle = PlatformHandle::new(Arc::new(EmptyCapabilities), events.clone(), facets);
    let mut client = PlatformClient::new(handle);
    let submission = client.submit_system_telemetry(1).expect("revision");
    let request_id = submission.cpu.expect("CPU request accepted");
    let sequence = EventSequence::new(8);
    let failure = operation_failure(
        request_id,
        CapabilityId::TELEMETRY_CPU,
        sequence,
        FailureKind::PermissionDenied,
        80,
    );
    events.push(EventEnvelope {
        request_id,
        capability: CapabilityId::TELEMETRY_CPU,
        provider: Some(ProviderId::borrowed("fixture.cpu")),
        sequence,
        observed_at_ms: 80,
        outcome: Err(failure),
    });

    let batch = client.try_drain().expect("event port remains live");

    let [outcome] = batch.system_telemetry_outcomes.as_slice() else {
        panic!("accepted failure should publish exactly one outcome");
    };
    assert_eq!(outcome.request_id, request_id);
    assert_eq!(outcome.capability, CapabilityId::TELEMETRY_CPU);
    assert_eq!(
        outcome.provider.as_ref().map(ProviderId::as_str),
        Some("fixture.cpu")
    );
    assert_eq!(outcome.sequence, sequence);
    assert_eq!(outcome.observed_at_ms, 80);
    assert!(matches!(
        outcome.event,
        SystemTelemetryDomainOutcome::Unavailable {
            revision,
            domain: SystemTelemetryDomain::Cpu,
            reason: SystemTelemetryUnavailable::Provider(FailureKind::PermissionDenied),
        } if revision == submission.revision
    ));
    assert_eq!(batch.failures.len(), 1);
}

#[test]
fn stale_old_runtime_failure_has_no_correlated_outcome() {
    let cpu = Arc::new(Accepting::<CpuTelemetryRequest>::default());
    let events = Arc::new(QueueEvents::default());
    let facets = PlatformFacets::default().with_system(SystemFacets::default().with_cpu(cpu));
    let handle = PlatformHandle::new(Arc::new(EmptyCapabilities), events.clone(), facets);
    let mut client = PlatformClient::new(handle);
    let first = client.submit_system_telemetry(1).expect("first revision");
    let stale_request = first.cpu.expect("first CPU request");
    let second = client.submit_system_telemetry(2).expect("second revision");
    assert_ne!(stale_request, second.cpu.expect("second CPU request"));
    let sequence = EventSequence::new(9);
    let failure = operation_failure(
        stale_request,
        CapabilityId::TELEMETRY_CPU,
        sequence,
        FailureKind::TimedOut,
        90,
    );
    events.push(EventEnvelope {
        request_id: stale_request,
        capability: CapabilityId::TELEMETRY_CPU,
        provider: Some(ProviderId::borrowed("fixture.cpu")),
        sequence,
        observed_at_ms: 90,
        outcome: Err(failure),
    });

    let batch = client.try_drain().expect("event port remains live");

    assert!(batch.system_telemetry_outcomes.is_empty());
    assert_eq!(batch.failures.len(), 1);
}

#[test]
fn accepted_observation_keeps_envelope_and_duplicate_is_not_republished() {
    let cpu = Arc::new(Accepting::<CpuTelemetryRequest>::default());
    let events = Arc::new(QueueEvents::default());
    let facets = PlatformFacets::default().with_system(SystemFacets::default().with_cpu(cpu));
    let handle = PlatformHandle::new(Arc::new(EmptyCapabilities), events.clone(), facets);
    let mut client = PlatformClient::new(handle);
    let submission = client.submit_system_telemetry(1).expect("revision");
    let request_id = submission.cpu.expect("CPU request accepted");
    let sequence = EventSequence::new(10);
    let event = EventEnvelope {
        request_id,
        capability: CapabilityId::TELEMETRY_CPU,
        provider: Some(ProviderId::borrowed("fixture.cpu")),
        sequence,
        observed_at_ms: 100,
        outcome: Ok(PlatformEvent::SystemTelemetry(
            SystemTelemetryDomainEvent::Cpu {
                revision: submission.revision,
                observation: Box::new(CpuTelemetryObservation::current(
                    CpuMetrics::from_observations(CpuScalarObservations {
                        global_usage_pct: ScalarObservation::available(42.0, 100),
                        ..Default::default()
                    }),
                    100,
                    Vec::new(),
                )),
            },
        )),
    };
    events.push(event.clone());
    events.push(event);

    let batch = client.try_drain().expect("event port remains live");

    let [outcome] = batch.system_telemetry_outcomes.as_slice() else {
        panic!("only the accepted completion should publish an outcome");
    };
    assert_eq!(outcome.request_id, request_id);
    assert_eq!(outcome.capability, CapabilityId::TELEMETRY_CPU);
    assert_eq!(outcome.sequence, sequence);
    assert_eq!(outcome.observed_at_ms, 100);
    let SystemTelemetryDomainOutcome::Observed(SystemTelemetryDomainEvent::Cpu {
        revision,
        observation,
    }) = &outcome.event
    else {
        panic!("expected observed CPU outcome");
    };
    assert_eq!(*revision, submission.revision);
    assert_eq!(
        observation
            .current_value()
            .and_then(CpuMetrics::current_global_usage_pct),
        Some(42.0)
    );
    assert_eq!(batch.failures.len(), 1, "duplicate raw event is diagnosed");
}

#[test]
fn submission_failures_never_fabricate_correlated_outcomes() {
    let handle = PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default(),
    );
    let mut client = PlatformClient::new(handle);

    let submission = client.submit_system_telemetry(1).expect("revision");
    assert!(!submission.has_pending_requests());
    let batch = client.try_drain().expect("empty event port remains live");

    assert!(batch.system_telemetry_outcomes.is_empty());
    assert!(batch.failures.is_empty());
}
