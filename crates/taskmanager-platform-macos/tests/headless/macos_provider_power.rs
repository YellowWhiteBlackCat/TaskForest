use super::*;

#[test]
fn power_refresh_has_coherent_discovery_authority() {
    let snapshot = MacPowerSupplyProvider
        .refresh(1)
        .expect("power provider returns a typed snapshot");
    taskmanager_platform_conformance::assert_device_discovery_consistent(&snapshot)
        .expect("macOS power discovery must be coherent");
}
