use taskmanager_core::core::metrics::MsrPackageReadout;
use taskmanager_platform_contract::CapabilityRequest;

use super::*;

#[test]
fn msr_readout_request_owns_the_telemetry_cpu_msr_capability() {
    assert_eq!(
        MsrReadoutRequest::CAPABILITY,
        CapabilityId::TELEMETRY_CPU_MSR
    );
}

#[test]
fn update_events_only_accept_the_telemetry_cpu_msr_capability() {
    let update = MsrReadoutEvent::Update(MsrReadoutSnapshot::success(vec![MsrPackageReadout {
        cpu: 0,
        bclk_mhz: None,
        temperature_c: Some(54.5),
        multiplier: Some(42.0),
        multiplier_min: Some(8.0),
        multiplier_max: Some(58.0),
        vcore_v: Some(1.219),
    }]));
    assert!(update.accepts_capability(&CapabilityId::TELEMETRY_CPU_MSR));
    assert!(!update.accepts_capability(&CapabilityId::TELEMETRY_CPU));
}
