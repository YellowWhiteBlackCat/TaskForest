use taskmanager_core::{SmartSelfTestReport, StorageDeviceKey};

use super::*;

fn observation(device: &str) -> SmartSelfTestObservation {
    SmartSelfTestObservation {
        device_id: DeviceId::new(format!("disk:{device}")),
        device_generation: DeviceGeneration::INITIAL,
        device_key: StorageDeviceKey::new(device),
        display_name: device.into(),
        report: SmartSelfTestReport::default(),
    }
}

#[test]
fn late_pre_cancel_batch_cannot_resurrect_a_newer_empty_projection() {
    let mut projection = SmartObservationProjection::default();
    assert_eq!(
        projection.apply(&SmartObservationBatch {
            revision: SmartStateRevision::new(2),
            ..SmartObservationBatch::default()
        }),
        SmartProjectionApplyResult::Applied
    );
    assert_eq!(
        projection.apply(&SmartObservationBatch {
            revision: SmartStateRevision::new(1),
            observations: vec![observation("old-poll")],
            ..SmartObservationBatch::default()
        }),
        SmartProjectionApplyResult::IgnoredStaleOrDuplicateRevision
    );
    assert!(projection.observations().is_empty());
}

#[test]
fn duplicate_target_batch_is_rejected_without_mutating_the_projection() {
    let mut projection = SmartObservationProjection::default();
    let duplicate = observation("duplicate");
    assert_eq!(
        projection.apply(&SmartObservationBatch {
            revision: SmartStateRevision::new(1),
            observations: vec![duplicate.clone(), duplicate],
            ..SmartObservationBatch::default()
        }),
        SmartProjectionApplyResult::RejectedDuplicateTarget
    );
    assert_eq!(projection.revision(), SmartStateRevision::default());
    assert!(projection.observations().is_empty());
}
