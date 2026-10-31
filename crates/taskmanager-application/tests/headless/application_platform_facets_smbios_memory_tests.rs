use taskforest_contract::CapabilityRequest;
use taskforest_core::core::metrics::SmbiosModuleRow;

use super::*;

#[test]
fn smbios_memory_request_owns_the_telemetry_memory_smbios_capability() {
    assert_eq!(
        SmbiosMemoryRequest::CAPABILITY,
        CapabilityId::TELEMETRY_MEMORY_SMBIOS
    );
}

#[test]
fn update_events_only_accept_the_telemetry_memory_smbios_capability() {
    let update = SmbiosMemoryEvent::Update(SmbiosMemorySnapshot::success(
        2,
        1,
        vec![SmbiosModuleRow {
            slot: 0,
            size_mb: Some(16_384),
            manufacturer: Some("Samsung".to_owned()),
            ..SmbiosModuleRow::default()
        }],
        None,
    ));
    assert!(update.accepts_capability(&CapabilityId::TELEMETRY_MEMORY_SMBIOS));
    assert!(!update.accepts_capability(&CapabilityId::TELEMETRY_MEMORY));
}
