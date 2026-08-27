use taskmanager_platform_contract::{CapabilityRequest, RequestTracking};

use super::*;
use crate::{DeviceGeneration, DeviceId, StorageDeviceKey};

fn target(generation: u64, locator: &str) -> StorageDeviceTarget {
    StorageDeviceTarget {
        device_id: DeviceId::new("disk:stable"),
        device_generation: DeviceGeneration::new(generation),
        locator: StorageDeviceKey::new(locator),
    }
}

#[test]
fn smart_target_scope_uses_physical_identity_and_generation_not_native_locator() {
    assert_eq!(
        SmartObservationRequest::RefreshAll.runtime_tracking(),
        Ok(RequestTracking::Capability)
    );
    let first = SmartObservationRequest::RefreshTarget(target(1, "/dev/nvme0"));
    let renumbered = SmartObservationRequest::RefreshTarget(target(1, "/dev/nvme7"));
    let replaced = SmartObservationRequest::RefreshTarget(target(2, "/dev/nvme7"));
    assert_eq!(first.runtime_tracking(), renumbered.runtime_tracking());
    assert_ne!(first.runtime_tracking(), replaced.runtime_tracking());
}

#[test]
fn smart_control_and_observation_share_the_same_target_scope() {
    let target = target(3, "/dev/nvme1");
    assert_eq!(
        SmartObservationRequest::RefreshTarget(target.clone()).runtime_tracking(),
        SmartControlRequest::StopTracking(target).runtime_tracking()
    );
}

#[test]
fn smart_target_scope_rejects_missing_physical_identity_or_generation() {
    for target in [
        StorageDeviceTarget {
            device_id: DeviceId::default(),
            device_generation: DeviceGeneration::INITIAL,
            locator: StorageDeviceKey::new("/dev/nvme0"),
        },
        StorageDeviceTarget {
            device_id: DeviceId::new("disk:stable"),
            device_generation: DeviceGeneration::default(),
            locator: StorageDeviceKey::new("/dev/nvme0"),
        },
    ] {
        assert_eq!(
            SmartObservationRequest::RefreshTarget(target).runtime_tracking(),
            Err(RequestTrackingError::MissingTargetIdentity)
        );
    }
}
