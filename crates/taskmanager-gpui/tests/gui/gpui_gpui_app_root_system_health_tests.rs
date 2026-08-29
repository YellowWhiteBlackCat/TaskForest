use taskmanager_application::{
    SmartObservationBatch, SmartObservationProjection, SmartProjectionApplyResult,
    SmartStateRevision,
};
use taskmanager_core::core::{
    DeviceGeneration, DeviceId, SmartSelfTestObservation, SmartSelfTestReport, StorageDeviceKey,
};

use super::smart_report_for_device;

fn observation(device: &str, generation: u64) -> SmartSelfTestObservation {
    SmartSelfTestObservation {
        device_id: DeviceId::new(format!("disk:{device}")),
        device_generation: DeviceGeneration::new(generation),
        device_key: StorageDeviceKey::new(device),
        display_name: device.into(),
        report: SmartSelfTestReport::default(),
    }
}

#[test]
fn smart_report_borrows_only_the_exact_visible_device_generation() {
    let mut projection = SmartObservationProjection::default();
    let disk_a = observation("a", 1);
    let disk_b = observation("b", 1);
    let first = SmartObservationBatch {
        revision: SmartStateRevision::new(1),
        subject: Some(disk_b.target()),
        observations: vec![disk_a.clone(), disk_b.clone()],
        ..SmartObservationBatch::default()
    };
    assert_eq!(
        projection.apply(&first),
        SmartProjectionApplyResult::Applied
    );
    assert!(smart_report_for_device(&projection, "disk:b", DeviceGeneration::new(1)).is_some());
    assert!(smart_report_for_device(&projection, "disk:b", DeviceGeneration::new(2)).is_none());

    let next = SmartObservationBatch {
        revision: SmartStateRevision::new(2),
        observations: vec![disk_a],
        ..SmartObservationBatch::default()
    };
    assert_eq!(projection.apply(&next), SmartProjectionApplyResult::Applied);
    assert!(smart_report_for_device(&projection, "disk:b", DeviceGeneration::new(1)).is_none());
    assert!(smart_report_for_device(&projection, "disk:a", DeviceGeneration::new(1)).is_some());

    let empty = SmartObservationBatch {
        revision: SmartStateRevision::new(3),
        ..SmartObservationBatch::default()
    };
    assert_eq!(
        projection.apply(&empty),
        SmartProjectionApplyResult::Applied
    );
    assert!(smart_report_for_device(&projection, "disk:a", DeviceGeneration::new(1)).is_none());
    assert_eq!(projection.revision(), SmartStateRevision::new(3));
}
