use taskmanager_platform_contract::CapabilityRequest;

use super::*;

#[test]
fn cpu_throttle_request_owns_the_telemetry_cpu_throttle_capability() {
    assert_eq!(
        CpuThrottleRequest::CAPABILITY,
        CapabilityId::TELEMETRY_CPU_THROTTLE
    );
}

#[test]
fn update_events_only_accept_the_telemetry_cpu_throttle_capability() {
    let update = CpuThrottleEvent::Update(CpuThrottleSnapshot::success(Vec::new()));
    assert!(update.accepts_capability(&CapabilityId::TELEMETRY_CPU_THROTTLE));
    assert!(!update.accepts_capability(&CapabilityId::TELEMETRY_CPU));
}
