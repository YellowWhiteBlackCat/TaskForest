//! test-intent: behavior
//!
//! Headless behavior tests for the PreUpdate drain seam.
//!
//! The drain contract is exercised against the REAL `PlatformClient` batch
//! reducer and the REAL shell fold, over scripted event ports — no window, no
//! bevy schedule, no compositor. The ports are independent recorders: their
//! drain/call counts are the oracle for "the seam drained exactly this much".

use std::sync::{Arc, Mutex};

use taskmanager_application::{
    PlatformClient, PlatformEvent, PlatformFacets, PlatformHandle, ShellEvent,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, EventSequence, RequestId,
};

use taskmanager_shell::ShellApp;

use super::{EVENT_DRAIN_BATCH, capability_summary_line, run_drain_cycle};

/// Capability catalog serving one fixed snapshot per handle.
struct FixedCapabilities(CapabilitySnapshot);

impl CapabilityCatalog for FixedCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        self.0.clone()
    }
}

/// Scripted event port. `mode` decides what `try_recv` answers; the shared
/// counters record every call so tests can assert how far the drain reached.
struct ScriptedEvents {
    mode: Mutex<PortMode>,
    recv_calls: Mutex<usize>,
    drained_events: Mutex<usize>,
}

enum PortMode {
    Quiet,
    Pending(Vec<EventEnvelope<PlatformEvent>>),
    /// Boxed: `EventEnvelope` is a large struct and the endless mode carries
    /// it on every probe (clippy::large_enum_variant).
    Endless(Box<EventEnvelope<PlatformEvent>>),
    Failing,
}

impl ScriptedEvents {
    fn quiet() -> Arc<Self> {
        Arc::new(Self {
            mode: Mutex::new(PortMode::Quiet),
            recv_calls: Mutex::new(0),
            drained_events: Mutex::new(0),
        })
    }

    fn pending(events: Vec<EventEnvelope<PlatformEvent>>) -> Arc<Self> {
        Arc::new(Self {
            mode: Mutex::new(PortMode::Pending(events)),
            recv_calls: Mutex::new(0),
            drained_events: Mutex::new(0),
        })
    }

    fn endless() -> Arc<Self> {
        Arc::new(Self {
            mode: Mutex::new(PortMode::Endless(Box::new(delivered_event(1)))),
            recv_calls: Mutex::new(0),
            drained_events: Mutex::new(0),
        })
    }

    fn failing() -> Arc<Self> {
        Arc::new(Self {
            mode: Mutex::new(PortMode::Failing),
            recv_calls: Mutex::new(0),
            drained_events: Mutex::new(0),
        })
    }

    fn recv_calls(&self) -> usize {
        *self.recv_calls.lock().expect("recv-call counter lock")
    }

    fn drained_events(&self) -> usize {
        *self.drained_events.lock().expect("drain counter lock")
    }

    fn next_envelope(sequence: u64) -> EventEnvelope<PlatformEvent> {
        delivered_event(sequence)
    }
}

impl EventPort for ScriptedEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        *self.recv_calls.lock().expect("recv-call counter lock") += 1;
        let mut mode = self.mode.lock().expect("port mode lock");
        match &mut *mode {
            PortMode::Quiet => Ok(None),
            PortMode::Pending(events) => {
                if events.is_empty() {
                    return Ok(None);
                }
                *self.drained_events.lock().expect("drain counter lock") += 1;
                Ok(Some(events.remove(0)))
            }
            PortMode::Endless(template) => {
                *self.drained_events.lock().expect("drain counter lock") += 1;
                let count = self.drained_events();
                let mut envelope = (**template).clone();
                envelope.sequence = EventSequence::new(u64::try_from(count).unwrap_or(u64::MAX));
                Ok(Some(envelope))
            }
            PortMode::Failing => Err(EventPortError::RuntimeStopped),
        }
    }
}

/// One `alerts.notify` delivery echo — the smallest well-formed payload that
/// survives the application reducer into a non-empty `PlatformEventBatch`.
fn delivered_event(sequence: u64) -> EventEnvelope<PlatformEvent> {
    EventEnvelope {
        request_id: RequestId::new(sequence.max(1)).expect("fixture request id"),
        capability: CapabilityId::DESKTOP_NOTIFY,
        provider: Some(ProviderId::borrowed("test.notify.echo")),
        sequence: EventSequence::new(sequence),
        observed_at_ms: 100,
        outcome: Ok(PlatformEvent::Shell(ShellEvent::NotificationDelivered)),
    }
}

fn descriptor(id: CapabilityId, status: CapabilityStatus) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id,
        status,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }
}

fn snapshot_with_mixed_statuses() -> CapabilitySnapshot {
    CapabilitySnapshot::from_descriptors([
        descriptor(CapabilityId::TELEMETRY_HOST, CapabilityStatus::Available),
        descriptor(CapabilityId::TELEMETRY_CPU, CapabilityStatus::Available),
        descriptor(
            CapabilityId::HARDWARE_INVENTORY,
            CapabilityStatus::PermissionRequired,
        ),
        descriptor(
            CapabilityId::CONTAINERS,
            CapabilityStatus::Degraded(FailureKind::TimedOut),
        ),
    ])
}

fn client_with(snapshot: CapabilitySnapshot, events: Arc<ScriptedEvents>) -> PlatformClient {
    PlatformClient::new(PlatformHandle::new(
        Arc::new(FixedCapabilities(snapshot)),
        events,
        PlatformFacets::default(),
    ))
}

/// Fold one drain cycle against a freshly built client; a warm-up cycle
/// first, so the capability fold is already at its steady state unless the
/// test says otherwise.
fn cycle_after_warmup(
    snapshot: CapabilitySnapshot,
    events: Arc<ScriptedEvents>,
) -> super::DrainCycle {
    let mut client = client_with(snapshot.clone(), events.clone());
    let mut shell = ShellApp::new();
    let _ = run_drain_cycle(&mut client, &mut shell, 0);
    run_drain_cycle(&mut client, &mut shell, 1_000)
}

#[test]
fn idle_cycle_does_nothing() {
    let events = ScriptedEvents::quiet();
    let cycle = cycle_after_warmup(snapshot_with_mixed_statuses(), events.clone());
    assert_eq!(cycle.folded_batches, 0, "a quiet port must fold nothing");
    assert!(
        cycle.capability_summary.is_none(),
        "an unchanged inventory must not re-emit the summary"
    );
    assert!(
        !cycle.refresh_submitted,
        "an idle frame with no scheduler must submit no refresh"
    );
    assert_eq!(
        events.recv_calls(),
        2,
        "two cycles against a quiet port must make exactly one probe each \
         (warm-up + measured), not spin the batch bound"
    );
}

#[test]
fn sustained_flood_folds_at_most_the_batch_bound() {
    let events = ScriptedEvents::endless();
    let cycle = cycle_after_warmup(snapshot_with_mixed_statuses(), events.clone());
    assert_eq!(
        cycle.folded_batches, EVENT_DRAIN_BATCH,
        "an endless port must fold exactly one bounded batch set per frame"
    );
    assert!(
        events.drained_events() > EVENT_DRAIN_BATCH,
        "the endless port must still hold events after the frame (the bound, \
         not exhaustion, ended the drain)"
    );
}

#[test]
fn pending_events_fold_once_then_the_port_is_quiet() {
    let events = ScriptedEvents::pending(vec![
        ScriptedEvents::next_envelope(1),
        ScriptedEvents::next_envelope(2),
        ScriptedEvents::next_envelope(3),
    ]);
    let mut client = client_with(snapshot_with_mixed_statuses(), events.clone());
    let mut shell = ShellApp::new();
    let first = run_drain_cycle(&mut client, &mut shell, 0);
    assert_eq!(
        first.folded_batches, 1,
        "three ready events drain inside one non-empty batch"
    );
    assert_eq!(
        events.drained_events(),
        3,
        "every scripted event is consumed"
    );
    let second = run_drain_cycle(&mut client, &mut shell, 1_000);
    assert_eq!(second.folded_batches, 0, "a drained port stays drained");
}

#[test]
fn port_failure_reports_one_notice_and_stops_draining() {
    let events = ScriptedEvents::failing();
    let mut client = client_with(snapshot_with_mixed_statuses(), events.clone());
    let mut shell = ShellApp::new();
    let cycle = run_drain_cycle(&mut client, &mut shell, 0);
    assert_eq!(cycle.folded_batches, 0);
    assert!(
        !shell.feedback_text().is_empty(),
        "a broken port must surface as shell feedback"
    );
    assert_eq!(
        events.recv_calls(),
        1,
        "a persistently broken port must not be retried sixteen times"
    );
}

#[test]
fn capability_summary_fires_once_per_inventory_change() {
    let events = ScriptedEvents::quiet();
    let mut client = client_with(snapshot_with_mixed_statuses(), events);
    let mut shell = ShellApp::new();
    let first = run_drain_cycle(&mut client, &mut shell, 0);
    assert!(
        first.capability_summary.is_some(),
        "the first fold of a non-empty inventory must publish a summary"
    );
    let second = run_drain_cycle(&mut client, &mut shell, 1_000);
    assert!(
        second.capability_summary.is_none(),
        "an identical inventory must not re-publish"
    );
}

#[test]
fn summary_line_counts_typed_statuses_without_fabricating_zero() {
    let snapshot = snapshot_with_mixed_statuses();
    let line = capability_summary_line(&snapshot);
    assert!(line.contains("4 capabilities"), "total count: {line}");
    assert!(line.contains("2 available"), "available count: {line}");
    assert!(
        line.contains("1 permission required"),
        "permission-required count: {line}"
    );
    assert!(
        line.contains("1 other states"),
        "degraded lands in the other-states bucket: {line}"
    );
    assert!(
        line.contains("0 unsupported"),
        "a true zero count of unsupported capabilities is honest arithmetic: {line}"
    );

    let empty = capability_summary_line(&CapabilitySnapshot::default());
    assert_eq!(
        empty, "platform runtime: no capability observations yet",
        "an empty inventory must say so explicitly, not report zero available"
    );
}
