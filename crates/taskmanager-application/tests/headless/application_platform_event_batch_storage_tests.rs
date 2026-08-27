use taskmanager_core::{DeviceGeneration, DeviceId, SmartSelfTestObservation, SmartSelfTestReport};
use taskmanager_platform_contract::{CapabilityId, RequestId};

use super::super::super::{PlatformEvent, SmartEvent};
use super::super::{PlatformEventBatch, test_support::test_event_context};

#[test]
fn smart_batch_preserves_physical_target_identity_separate_from_native_locator() {
    let mut batch = PlatformEventBatch::default();
    let request_id = RequestId::new(6).expect("non-zero fixture request");
    batch.merge(
        test_event_context(request_id, CapabilityId::SMART),
        PlatformEvent::Smart(SmartEvent::Batch(
            super::super::super::SmartObservationBatch {
                observations: vec![SmartSelfTestObservation {
                    device_id: DeviceId::new("disk:wwid:fixture"),
                    device_generation: DeviceGeneration::new(4),
                    device_key: "nvme0n1".into(),
                    display_name: "Fixture NVMe".into(),
                    report: SmartSelfTestReport::default(),
                }],
                ..Default::default()
            },
        )),
    );

    let event = batch
        .smart_events
        .first()
        .expect("SMART event should be retained");
    let SmartEvent::Batch(smart_batch) = &event.event;
    let Some(observation) = smart_batch.observations.first() else {
        panic!("expected target-keyed SMART observation");
    };
    assert_eq!(event.request_id, request_id);
    assert_eq!(observation.device_id.as_str(), "disk:wwid:fixture");
    assert_eq!(observation.device_generation.get(), 4);
    assert_eq!(observation.device_key.as_str(), "nvme0n1");
}
