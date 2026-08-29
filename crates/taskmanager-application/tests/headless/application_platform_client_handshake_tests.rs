use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityId, CapabilitySnapshot, CompositeSourceSnapshot, EventEnvelope,
    EventPort, EventPortError, EventSequence, OperationFailure, ProviderFailure, RequestEnvelope,
    RequestId, RequestPort, SubmissionError,
};

use crate::platform::{
    DesktopAppearanceEvent, DesktopAppearanceRequest, IntegrationFacets, PlatformClient,
    PlatformEvent, PlatformFacets, PlatformHandle,
};
use taskmanager_core::core::appearance::{DesktopAppearance, DesktopFamily, PreferredColorScheme};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};

use super::submitted_at_ms;

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

/// Port that answers every accepted request with a correlated snapshot
/// envelope, the way a real native adapter does.
struct RespondingAppearance {
    events: Arc<QueuedEvents>,
    response: DesktopAppearance,
}

impl RequestPort for RespondingAppearance {
    type Request = DesktopAppearanceRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.events.push(snapshot_event(request.id, self.response));
        Ok(())
    }
}

/// Port that accepts every request but never answers (the handshake must
/// fall through to its own timeout).
struct SilentAppearance;

impl RequestPort for SilentAppearance {
    type Request = DesktopAppearanceRequest;

    fn try_submit(&self, _request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        Ok(())
    }
}

/// Port that answers every accepted request with a correlated failure
/// receipt.
struct FailingAppearance {
    events: Arc<QueuedEvents>,
}

impl RequestPort for FailingAppearance {
    type Request = DesktopAppearanceRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.events.push(failed_event(request.id));
        Ok(())
    }
}

fn snapshot_event(request_id: RequestId, value: DesktopAppearance) -> EventEnvelope<PlatformEvent> {
    EventEnvelope {
        request_id,
        capability: CapabilityId::DESKTOP_APPEARANCE,
        provider: Some(ProviderId::borrowed("test.desktop.appearance")),
        sequence: EventSequence::new(1),
        observed_at_ms: 100,
        outcome: Ok(PlatformEvent::DesktopAppearance(
            DesktopAppearanceEvent::Snapshot(CompositeSourceSnapshot {
                value,
                sources: vec![SourceStatus {
                    provider: ProviderId::borrowed("test.desktop.appearance"),
                    outcome: SourceOutcome::Available,
                    item_count: 1,
                }],
            }),
        )),
    }
}

fn failed_event(request_id: RequestId) -> EventEnvelope<PlatformEvent> {
    let kind = FailureKind::TimedOut;
    EventEnvelope {
        request_id,
        capability: CapabilityId::DESKTOP_APPEARANCE,
        provider: Some(ProviderId::borrowed("test.desktop.appearance")),
        sequence: EventSequence::new(1),
        observed_at_ms: 100,
        outcome: Err(OperationFailure {
            request_id,
            capability: CapabilityId::DESKTOP_APPEARANCE,
            sequence: EventSequence::new(1),
            kind,
            retry: ProviderFailure::from_kind(kind).retry(),
            provider: Some(ProviderId::borrowed("test.desktop.appearance")),
            observed_at_ms: 100,
        }),
    }
}

fn client(events: Arc<QueuedEvents>) -> PlatformClient {
    PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        events,
        PlatformFacets::default(),
    ))
}

fn answering_client(events: Arc<QueuedEvents>, response: DesktopAppearance) -> PlatformClient {
    let queue = events.clone();
    let integration =
        IntegrationFacets::default().with_desktop_appearance(Arc::new(RespondingAppearance {
            events: queue,
            response,
        }));
    PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        events,
        PlatformFacets::default().with_integration(integration),
    ))
}

fn failing_client(events: Arc<QueuedEvents>) -> PlatformClient {
    let queue = events.clone();
    let integration = IntegrationFacets::default()
        .with_desktop_appearance(Arc::new(FailingAppearance { events: queue }));
    PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        events,
        PlatformFacets::default().with_integration(integration),
    ))
}

fn appearance() -> DesktopAppearance {
    DesktopAppearance {
        family: DesktopFamily::Kde,
        color_scheme: PreferredColorScheme::Dark,
        high_contrast: Some(false),
    }
}

#[test]
fn matched_snapshot_returns_the_typed_value_and_sources() {
    let events = Arc::new(QueuedEvents::default());
    let expected = appearance();
    let mut client = answering_client(events, expected);

    let handshake = client.observe_desktop_appearance(
        std::time::Duration::from_secs(1),
        std::time::Duration::from_millis(1),
    );

    let snapshot = handshake
        .snapshot
        .expect("matched snapshot must be returned");
    assert_eq!(snapshot.value, expected);
    assert_eq!(snapshot.sources.len(), 1);
    assert_eq!(snapshot.sources[0].outcome, SourceOutcome::Available);
    assert!(handshake.failures.is_empty());
}

#[test]
fn matched_failure_receipt_is_kept_and_no_snapshot_returned() {
    let events = Arc::new(QueuedEvents::default());
    let mut client = failing_client(events);

    let handshake = client.observe_desktop_appearance(
        std::time::Duration::from_secs(1),
        std::time::Duration::from_millis(1),
    );

    assert!(handshake.snapshot.is_none());
    assert_eq!(handshake.failures.len(), 1);
    assert_eq!(handshake.failures[0].kind, FailureKind::TimedOut);
}

#[test]
fn missing_capability_returns_default_without_failures() {
    let events = Arc::new(QueuedEvents::default());
    let mut client = client(events);

    let handshake = client.observe_desktop_appearance(
        std::time::Duration::from_secs(1),
        std::time::Duration::from_millis(1),
    );

    assert!(handshake.snapshot.is_none());
    assert!(handshake.failures.is_empty());
}

#[test]
fn stale_event_for_another_request_does_not_complete_the_handshake() {
    let events = Arc::new(QueuedEvents::default());
    let queue = events.clone();
    let integration =
        IntegrationFacets::default().with_desktop_appearance(Arc::new(SilentAppearance));
    let mut client = PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        events,
        PlatformFacets::default().with_integration(integration),
    ));
    let request_id = client
        .submit_desktop_appearance(DesktopAppearanceRequest::Observe, submitted_at_ms())
        .expect("submission accepted");
    // A snapshot for a DIFFERENT request id: the loop must ignore it and
    // keep waiting (falls through to the timeout fallback).
    let stale = RequestId::new(request_id.get().wrapping_add(7)).expect("different non-zero id");
    queue.push(snapshot_event(stale, appearance()));

    let handshake = client.observe_desktop_appearance(
        std::time::Duration::from_millis(20),
        std::time::Duration::from_millis(1),
    );

    assert!(
        handshake.snapshot.is_none(),
        "stale event must not complete"
    );
    assert!(handshake.failures.is_empty());
}

#[test]
fn timeout_returns_default_without_failures() {
    let events = Arc::new(QueuedEvents::default());
    let mut client = client(events);

    let handshake = client.observe_desktop_appearance(
        std::time::Duration::from_millis(20),
        std::time::Duration::from_millis(1),
    );

    assert!(handshake.snapshot.is_none());
    assert!(handshake.failures.is_empty());
}
