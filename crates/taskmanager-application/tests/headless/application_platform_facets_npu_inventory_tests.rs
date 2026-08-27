use taskmanager_core::DeviceId;
use taskmanager_platform_contract::CapabilityRequest;

use super::*;

#[test]
fn npu_inventory_request_owns_the_accelerator_npu_capability() {
    assert_eq!(
        NpuInventoryRequest::CAPABILITY,
        CapabilityId::ACCELERATOR_NPU
    );
}

#[test]
fn update_events_only_accept_the_accelerator_npu_capability() {
    let update = NpuInventoryEvent::Update(NpuInventorySnapshot::discovered(
        vec![taskmanager_core::NpuDevice {
            device_id: DeviceId::new("accel0"),
            driver: Some("intel_vpu".to_owned()),
            ..taskmanager_core::NpuDevice::default()
        }],
        7,
    ));
    assert!(update.accepts_capability(&CapabilityId::ACCELERATOR_NPU));
    assert!(!update.accepts_capability(&CapabilityId::TELEMETRY_GPU));
}
