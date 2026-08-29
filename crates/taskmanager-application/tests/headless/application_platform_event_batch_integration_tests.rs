use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{DesktopAppearance, DesktopFamily};
use taskmanager_platform_contract::{CapabilityId, CompositeSourceSnapshot, RequestId};

use super::super::super::{DesktopAppearanceEvent, PlatformEvent};
use super::super::{PlatformEventBatch, test_support::test_event_context};

#[test]
fn desktop_appearance_event_keeps_native_source_truth() {
    let mut batch = PlatformEventBatch::default();
    batch.merge(
        test_event_context(
            RequestId::new(9).expect("non-zero fixture request"),
            CapabilityId::DESKTOP_APPEARANCE,
        ),
        PlatformEvent::DesktopAppearance(DesktopAppearanceEvent::Snapshot(
            CompositeSourceSnapshot::new(
                DesktopAppearance {
                    family: DesktopFamily::Kde,
                    ..DesktopAppearance::default()
                },
                vec![SourceStatus {
                    provider: ProviderId::borrowed("fixture.desktop.session"),
                    outcome: SourceOutcome::Available,
                    item_count: 1,
                }],
            ),
        )),
    );

    let event = batch
        .desktop_appearance_events
        .first()
        .expect("appearance event should be retained");
    let DesktopAppearanceEvent::Snapshot(snapshot) = &event.event;
    assert_eq!(snapshot.value.family, DesktopFamily::Kde);
    assert_eq!(
        snapshot.sources[0].provider.as_str(),
        "fixture.desktop.session"
    );
}
