use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_platform_contract::{CapabilityId, PartialSourceSnapshot, RequestId};

use super::super::super::{PlatformEvent, ServiceEvent};
use super::super::{PlatformEventBatch, test_support::test_event_context};

#[test]
fn service_event_preserves_selected_runtime_provider() {
    let mut batch = PlatformEventBatch::default();

    batch.merge(
        test_event_context(
            RequestId::new(5).expect("non-zero fixture request"),
            CapabilityId::SERVICES,
        ),
        PlatformEvent::Services(ServiceEvent::Snapshot(PartialSourceSnapshot::new(
            Vec::new(),
            vec![SourceStatus {
                provider: ProviderId::borrowed("fixture.service.openrc"),
                outcome: SourceOutcome::Empty,
                item_count: 0,
            }],
        ))),
    );

    let event = batch
        .service_events
        .first()
        .expect("service event should be retained");
    let ServiceEvent::Snapshot(snapshot) = &event.event else {
        panic!("expected service snapshot");
    };
    assert!(snapshot.items.is_empty());
    assert_eq!(
        snapshot.sources[0].provider.as_str(),
        "fixture.service.openrc"
    );
}
