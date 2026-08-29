use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::{DeviceState, FailureKind, StartupBootEvidenceSnapshot, StartupFailedUnit};
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityId, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
    EventSequence, OperationFailure, ProviderFailure, RequestEnvelope, RequestPort,
    SubmissionError,
};

use crate::platform::{
    EnvironmentFacets, PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle,
    StartupEvidenceEvent, StartupEvidenceRequest, StartupEvidenceUnavailable,
};

#[derive(Default)]
struct EmptyCapabilities;

impl CapabilityCatalog for EmptyCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::default()
    }
}

#[derive(Default)]
struct Queue(Mutex<VecDeque<EventEnvelope<PlatformEvent>>>);

impl Queue {
    fn push(&self, event: EventEnvelope<PlatformEvent>) {
        self.0.lock().expect("event queue lock").push_back(event);
    }
}

impl EventPort for Queue {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(self.0.lock().expect("event queue lock").pop_front())
    }
}

#[derive(Default)]
struct AcceptingEvidence;

impl RequestPort for AcceptingEvidence {
    type Request = StartupEvidenceRequest;

    fn try_submit(&self, _request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        Ok(())
    }
}

fn client(events: Arc<Queue>) -> PlatformClient {
    PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        events,
        PlatformFacets::default().with_environment(
            EnvironmentFacets::default().with_startup_evidence(Arc::new(AcceptingEvidence)),
        ),
    ))
}

fn snapshot(now_ms: u64) -> StartupBootEvidenceSnapshot {
    StartupBootEvidenceSnapshot {
        state: DeviceState::healthy(now_ms),
        failed_units_state: DeviceState::healthy(now_ms),
        critical_chain_state: DeviceState::healthy(now_ms),
        failed_units: vec![StartupFailedUnit {
            unit: "broken.service".into(),
            load_state: "loaded".into(),
            active_state: "failed".into(),
            sub_state: "failed".into(),
            description: "Broken worker".into(),
        }],
        ..StartupBootEvidenceSnapshot::default()
    }
}

fn success(
    request_id: taskmanager_platform_contract::RequestId,
    sequence: u64,
    observed_at_ms: u64,
) -> EventEnvelope<PlatformEvent> {
    EventEnvelope {
        request_id,
        capability: CapabilityId::STARTUP_EVIDENCE,
        provider: Some(ProviderId::borrowed("fixture.startup.evidence")),
        sequence: EventSequence::new(sequence),
        observed_at_ms,
        outcome: Ok(PlatformEvent::StartupEvidence(
            StartupEvidenceEvent::Snapshot(snapshot(observed_at_ms)),
        )),
    }
}

#[test]
fn raw_startup_evidence_terminates_in_the_application_projection() {
    let events = Arc::new(Queue::default());
    let mut client = client(events.clone());
    let request_id = client
        .submit_startup_evidence(StartupEvidenceRequest::Refresh, 10)
        .expect("startup evidence request");
    events.push(success(request_id, 1, 20));

    let batch = client.try_drain().expect("drain startup evidence");
    assert_eq!(batch.startup_evidence_projections.len(), 1);
    assert!(batch.failures.is_empty());
    assert_eq!(
        batch.startup_evidence_projections[0]
            .snapshot
            .failed_units
            .len(),
        1
    );
}

#[test]
fn superseded_request_cannot_publish_after_a_newer_submission() {
    let events = Arc::new(Queue::default());
    let mut client = client(events.clone());
    let old = client
        .submit_startup_evidence(StartupEvidenceRequest::Refresh, 10)
        .expect("old request");
    let current = client
        .submit_startup_evidence(StartupEvidenceRequest::Refresh, 11)
        .expect("current request");
    events.push(success(old, 1, 20));
    events.push(success(current, 2, 21));

    let batch = client.try_drain().expect("drain both completions");
    assert_eq!(batch.startup_evidence_projections.len(), 1);
    assert_eq!(batch.startup_evidence_projections[0].revision.get(), 2);
    assert_eq!(batch.failures.len(), 1);
    assert_eq!(batch.failures[0].kind, FailureKind::Rejected);
}

#[test]
fn accepted_provider_failure_retains_last_successful_evidence() {
    let events = Arc::new(Queue::default());
    let mut client = client(events.clone());
    let first = client
        .submit_startup_evidence(StartupEvidenceRequest::Refresh, 10)
        .expect("first request");
    events.push(success(first, 1, 20));
    let first_batch = client.try_drain().expect("first completion");
    assert_eq!(first_batch.startup_evidence_projections.len(), 1);

    let failed = client
        .submit_startup_evidence(StartupEvidenceRequest::Refresh, 30)
        .expect("failed request");
    events.push(EventEnvelope {
        request_id: failed,
        capability: CapabilityId::STARTUP_EVIDENCE,
        provider: Some(ProviderId::borrowed("fixture.startup.evidence")),
        sequence: EventSequence::new(2),
        observed_at_ms: 40,
        outcome: Err(OperationFailure {
            request_id: failed,
            capability: CapabilityId::STARTUP_EVIDENCE,
            sequence: EventSequence::new(2),
            kind: FailureKind::TimedOut,
            retry: ProviderFailure::from_kind(FailureKind::TimedOut).retry(),
            provider: Some(ProviderId::borrowed("fixture.startup.evidence")),
            observed_at_ms: 40,
        }),
    });

    let failed_batch = client.try_drain().expect("failed completion");
    assert_eq!(failed_batch.startup_evidence_projections.len(), 1);
    let projected = &failed_batch.startup_evidence_projections[0];
    assert_eq!(projected.snapshot.failed_units.len(), 1);
    assert_eq!(
        projected.unavailable,
        Some(StartupEvidenceUnavailable::Provider(FailureKind::TimedOut))
    );
}
