use taskmanager_core::SessionItem;
use taskmanager_platform_contract::{
    CapabilityId, FailureKind, PartialSourceSnapshot, ProviderId, RequestId, SourceOutcome,
    SourceStatus,
};

use super::super::super::{PlatformEvent, SessionEvent, StartupEvent};
use super::super::{PlatformEventBatch, test_support::test_event_context};

#[test]
fn startup_event_preserves_partial_source_status() {
    let mut batch = PlatformEventBatch::default();

    batch.merge(
        test_event_context(
            RequestId::new(3).expect("non-zero fixture request"),
            CapabilityId::STARTUP,
        ),
        PlatformEvent::Startup(StartupEvent::Snapshot(PartialSourceSnapshot::new(
            Vec::new(),
            vec![SourceStatus {
                provider: ProviderId::borrowed("fixture.startup.systemd"),
                outcome: SourceOutcome::Unavailable(FailureKind::TimedOut),
                item_count: 0,
            }],
        ))),
    );

    let event = batch
        .startup_events
        .first()
        .expect("startup event should be retained");
    let StartupEvent::Snapshot(snapshot) = &event.event else {
        panic!("expected startup snapshot");
    };
    assert!(snapshot.items.is_empty());
    assert_eq!(
        snapshot.sources[0].outcome,
        SourceOutcome::Unavailable(FailureKind::TimedOut)
    );
}

#[test]
fn session_event_preserves_authoritative_empty_source() {
    let mut batch = PlatformEventBatch::default();

    batch.merge(
        test_event_context(
            RequestId::new(4).expect("non-zero fixture request"),
            CapabilityId::SESSIONS,
        ),
        PlatformEvent::Sessions(SessionEvent::Snapshot(PartialSourceSnapshot::new(
            Vec::<SessionItem>::new(),
            vec![SourceStatus {
                provider: ProviderId::borrowed("fixture.session.logind"),
                outcome: SourceOutcome::Empty,
                item_count: 0,
            }],
        ))),
    );

    let event = batch
        .session_events
        .first()
        .expect("session event should be retained");
    let SessionEvent::Snapshot(snapshot) = &event.event else {
        panic!("expected session snapshot");
    };
    assert!(snapshot.items.is_empty());
    assert_eq!(snapshot.sources[0].outcome, SourceOutcome::Empty);
}
