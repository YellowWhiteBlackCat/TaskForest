use taskmanager_core::GpuEngineMetric;
use taskmanager_platform_contract::CapabilityRequest;

use super::*;

#[test]
fn gpu_engine_rows_request_owns_the_telemetry_gpu_engines_capability() {
    assert_eq!(
        GpuEngineRowsRequest::CAPABILITY,
        CapabilityId::TELEMETRY_GPU_ENGINES
    );
}

#[test]
fn update_events_only_accept_the_telemetry_gpu_engines_capability() {
    let update = GpuEngineRowsEvent::Update(GpuEngineRowsSnapshot::success(
        DeviceId::new("gpu:0"),
        vec![GpuEngineMetric {
            name: "Render Ring".to_owned(),
            kind: taskmanager_core::GpuEngineKind::Unknown,
            utilization_pct: 12.5,
        }],
    ));
    assert!(update.accepts_capability(&CapabilityId::TELEMETRY_GPU_ENGINES));
    assert!(!update.accepts_capability(&CapabilityId::TELEMETRY_GPU));
}
