use taskmanager_core::core::metrics::RaplPackageRow;
use taskmanager_platform_contract::CapabilityRequest;

use super::*;

#[test]
fn rapl_power_request_owns_the_telemetry_cpu_package_power_capability() {
    assert_eq!(
        RaplPowerRequest::CAPABILITY,
        CapabilityId::TELEMETRY_CPU_PACKAGE_POWER
    );
}

#[test]
fn update_events_only_accept_the_telemetry_cpu_package_power_capability() {
    let update = RaplPowerEvent::Update(RaplPowerSnapshot::success(
        250,
        vec![RaplPackageRow {
            name: "package-1".to_owned(),
            power_w: 7.5,
            energy_delta_uj: 1_875_000,
        }],
    ));
    assert!(update.accepts_capability(&CapabilityId::TELEMETRY_CPU_PACKAGE_POWER));
    assert!(!update.accepts_capability(&CapabilityId::TELEMETRY_CPU));
}
