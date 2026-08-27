use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityId, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
    EventSequence, OperationFailure, ProviderFailure, ProviderId, RequestEnvelope, RequestPort,
    SubmissionError,
};

use crate::platform::{
    PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle, ProcessFacets,
    ProcessGpuRequest, ProcessInsightFacetEvent, ProcessInsightFacetState,
    ProcessInsightObservation, ProcessInsightUnavailable, ProcessNetworkRequest,
};
use crate::{
    FailureKind, FrozenProcessIdentity, ProcessGpuSnapshot, ProcessIdentity,
    ProcessInsightSnapshot, ProcessNetworkSnapshot,
};

#[derive(Default)]
struct EmptyCapabilities;

impl CapabilityCatalog for EmptyCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::default()
    }
}

#[derive(Default)]
struct QueuedEvents(Mutex<VecDeque<EventEnvelope<PlatformEvent>>>);

impl QueuedEvents {
    fn push(&self, event: EventEnvelope<PlatformEvent>) {
        self.0.lock().expect("event queue lock").push_back(event);
    }
}

impl EventPort for QueuedEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(self.0.lock().expect("event queue lock").pop_front())
    }
}

#[derive(Default)]
struct AcceptingNetwork(Mutex<Vec<ProcessNetworkRequest>>);

impl RequestPort for AcceptingNetwork {
    type Request = ProcessNetworkRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0
            .lock()
            .expect("network request lock")
            .push(request.payload);
        Ok(())
    }
}

#[derive(Default)]
struct AcceptingGpu(Mutex<Vec<ProcessGpuRequest>>);

impl RequestPort for AcceptingGpu {
    type Request = ProcessGpuRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0
            .lock()
            .expect("GPU request lock")
            .push(request.payload);
        Ok(())
    }
}

fn target() -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(42, "worker", 7_500, 9_000)
        .expect("fixture identity")
}

fn client(
    events: Arc<QueuedEvents>,
    network: Arc<AcceptingNetwork>,
    gpu: Option<Arc<AcceptingGpu>>,
) -> PlatformClient {
    let mut process = ProcessFacets::default().with_network(network);
    if let Some(gpu) = gpu {
        process = process.with_gpu(gpu);
    }
    PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        events,
        PlatformFacets::default().with_process(process),
    ))
}

fn successful_event(
    request_id: taskmanager_platform_contract::RequestId,
    capability: CapabilityId,
    payload: PlatformEvent,
    sequence: u64,
) -> EventEnvelope<PlatformEvent> {
    EventEnvelope {
        request_id,
        capability,
        provider: Some(ProviderId::borrowed("test.process.facet")),
        sequence: EventSequence::new(sequence),
        observed_at_ms: 100,
        outcome: Ok(payload),
    }
}

fn failed_event(
    request_id: taskmanager_platform_contract::RequestId,
    envelope_capability: CapabilityId,
    failure_capability: CapabilityId,
    kind: FailureKind,
) -> EventEnvelope<PlatformEvent> {
    EventEnvelope {
        request_id,
        capability: envelope_capability,
        provider: Some(ProviderId::borrowed("test.process.facet")),
        sequence: EventSequence::new(1),
        observed_at_ms: 100,
        outcome: Err(OperationFailure {
            request_id,
            capability: failure_capability,
            sequence: EventSequence::new(1),
            kind,
            retry: ProviderFailure::from_kind(kind).retry(),
            provider: Some(ProviderId::borrowed("test.process.facet")),
            observed_at_ms: 100,
        }),
    }
}

#[test]
fn stale_facet_success_is_a_typed_diagnostic_not_a_raw_frontend_event() {
    let events = Arc::new(QueuedEvents::default());
    let network = Arc::new(AcceptingNetwork::default());
    let mut client = client(events.clone(), network, None);
    let target = target();
    let submission = client
        .submit_process_insights(target.clone(), 10)
        .expect("submit process facets");
    let request_id = submission.network.expect("network request accepted");
    let stale_revision = submission
        .revision
        .checked_next()
        .expect("fixture revision has a successor");
    events.push(successful_event(
        request_id,
        CapabilityId::PROCESS_INSIGHTS_NETWORK,
        PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Network(Box::new(
            ProcessInsightObservation {
                target,
                revision: stale_revision,
                snapshot: ProcessInsightSnapshot {
                    identity: ProcessIdentity {
                        pid: 42,
                        start_token: 9_000,
                    },
                    value: ProcessNetworkSnapshot::default(),
                },
            },
        ))),
        1,
    ));

    let batch = client.try_drain().expect("drain stale facet event");
    assert!(batch.process_events.is_empty());
    assert_eq!(batch.process_insight_projections.len(), 1);
    assert_eq!(batch.failures.len(), 1);
    assert_eq!(batch.failures[0].kind, FailureKind::IdentityChanged);
    assert_eq!(batch.failures[0].request_id, request_id);
    assert_eq!(
        batch.failures[0].capability,
        CapabilityId::PROCESS_INSIGHTS_NETWORK
    );
    assert_eq!(batch.failures[0].sequence, EventSequence::new(1));
    assert_eq!(
        batch.failures[0].provider.as_ref().map(ProviderId::as_str),
        Some("test.process.facet")
    );
    assert_eq!(
        batch.process_insight_projections[0].network,
        ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
            FailureKind::IdentityChanged
        ))
    );
}

#[test]
fn conflicting_raw_identity_is_diagnosed_and_only_projection_leaves_the_reducer() {
    let events = Arc::new(QueuedEvents::default());
    let network = Arc::new(AcceptingNetwork::default());
    let gpu = Arc::new(AcceptingGpu::default());
    let mut client = client(events.clone(), network, Some(gpu));
    let target = target();
    let submission = client
        .submit_process_insights(target.clone(), 10)
        .expect("submit process facets");
    let network_id = submission.network.expect("network request accepted");
    let gpu_id = submission.gpu.expect("GPU request accepted");
    events.push(successful_event(
        network_id,
        CapabilityId::PROCESS_INSIGHTS_NETWORK,
        PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Network(Box::new(
            ProcessInsightObservation {
                target: target.clone(),
                revision: submission.revision,
                snapshot: ProcessInsightSnapshot {
                    identity: ProcessIdentity {
                        pid: 42,
                        start_token: 9_000,
                    },
                    value: ProcessNetworkSnapshot::default(),
                },
            },
        ))),
        1,
    ));
    events.push(successful_event(
        gpu_id,
        CapabilityId::PROCESS_INSIGHTS_GPU,
        PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Gpu(Box::new(
            ProcessInsightObservation {
                target,
                revision: submission.revision,
                snapshot: ProcessInsightSnapshot {
                    identity: ProcessIdentity {
                        pid: 42,
                        start_token: 9_001,
                    },
                    value: ProcessGpuSnapshot::default(),
                },
            },
        ))),
        2,
    ));

    let batch = client.try_drain().expect("drain conflicting facet events");
    assert!(batch.process_events.is_empty());
    assert_eq!(batch.process_insight_projections.len(), 2);
    assert_eq!(batch.failures.len(), 1);
    assert_eq!(batch.failures[0].kind, FailureKind::IdentityChanged);
    assert_eq!(
        batch.process_insight_projections[1].gpu,
        ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
            FailureKind::IdentityChanged
        ))
    );
}

#[test]
fn successful_payload_with_wrong_envelope_capability_fails_closed() {
    let events = Arc::new(QueuedEvents::default());
    let network = Arc::new(AcceptingNetwork::default());
    let mut client = client(events.clone(), network, None);
    let target = target();
    let submission = client
        .submit_process_insights(target.clone(), 10)
        .expect("submit process facets");
    let request_id = submission.network.expect("network request accepted");
    events.push(successful_event(
        request_id,
        CapabilityId::PROCESS_INSIGHTS_GPU,
        PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Network(Box::new(
            ProcessInsightObservation {
                target,
                revision: submission.revision,
                snapshot: ProcessInsightSnapshot {
                    identity: ProcessIdentity {
                        pid: 42,
                        start_token: 9_000,
                    },
                    value: ProcessNetworkSnapshot::default(),
                },
            },
        ))),
        1,
    ));

    let batch = client.try_drain().expect("drain mismatched facet event");
    assert!(batch.process_events.is_empty());
    assert_eq!(batch.failures.len(), 1);
    assert_eq!(batch.failures[0].kind, FailureKind::ProviderFault);
    assert_eq!(
        batch.process_insight_projections[0].network,
        ProcessInsightFacetState::Unavailable(ProcessInsightUnavailable::Provider(
            FailureKind::ProviderFault
        ))
    );
}

#[test]
fn direct_event_port_cannot_route_a_payload_under_an_unrelated_capability() {
    let events = Arc::new(QueuedEvents::default());
    let network = Arc::new(AcceptingNetwork::default());
    let mut client = client(events.clone(), network, None);
    let request_id = taskmanager_platform_contract::RequestId::new(70).expect("fixture id");
    events.push(successful_event(
        request_id,
        CapabilityId::PROCESS_LIST,
        PlatformEvent::Shell(crate::ShellEvent::TargetOpened),
        7,
    ));

    let batch = client.try_drain().expect("drain mismatched generic event");

    assert!(batch.shell_events.is_empty());
    assert!(batch.process_events.is_empty());
    assert_eq!(batch.failures.len(), 1);
    assert_eq!(batch.failures[0].request_id, request_id);
    assert_eq!(batch.failures[0].capability, CapabilityId::PROCESS_LIST);
    assert_eq!(batch.failures[0].kind, FailureKind::ProviderFault);
}

#[test]
fn failed_facet_requires_matching_envelope_and_failure_capabilities() {
    let events = Arc::new(QueuedEvents::default());
    let network = Arc::new(AcceptingNetwork::default());
    let mut client = client(events.clone(), network, None);
    let submission = client
        .submit_process_insights(target(), 10)
        .expect("submit process facets");
    let request_id = submission.network.expect("network request accepted");
    events.push(failed_event(
        request_id,
        CapabilityId::PROCESS_INSIGHTS_GPU,
        CapabilityId::PROCESS_INSIGHTS_NETWORK,
        FailureKind::TimedOut,
    ));

    let batch = client.try_drain().expect("drain mismatched facet failure");
    assert_eq!(batch.failures.len(), 1);
    assert_eq!(batch.failures[0].kind, FailureKind::ProviderFault);
    assert!(batch.process_insight_projections.is_empty());
}

#[test]
fn failed_event_with_mismatched_provider_is_rejected_before_pending_projection() {
    let events = Arc::new(QueuedEvents::default());
    let network = Arc::new(AcceptingNetwork::default());
    let mut client = client(events.clone(), network, None);
    let submission = client
        .submit_process_insights(target(), 10)
        .expect("submit process facets");
    let request_id = submission.network.expect("network request accepted");
    let mut event = failed_event(
        request_id,
        CapabilityId::PROCESS_INSIGHTS_NETWORK,
        CapabilityId::PROCESS_INSIGHTS_NETWORK,
        FailureKind::TimedOut,
    );
    if let Err(failure) = &mut event.outcome {
        failure.provider = Some(ProviderId::borrowed("test.other.provider"));
    }
    events.push(event);

    let batch = client.try_drain().expect("drain malformed failure event");
    assert!(batch.process_insight_projections.is_empty());
    assert_eq!(batch.failures.len(), 1);
    assert_eq!(batch.failures[0].kind, FailureKind::ProviderFault);
}
