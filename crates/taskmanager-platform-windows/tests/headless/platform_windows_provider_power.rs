use super::*;
use taskmanager_platform_conformance::assert_device_discovery_consistent;

#[test]
fn power_refresh_has_coherent_discovery_authority() {
    let snapshot = WinPowerSupplyProvider
        .refresh(1)
        .expect("power provider returns a typed snapshot");
    assert_device_discovery_consistent(&snapshot)
        .expect("Windows power discovery must be coherent");
}
