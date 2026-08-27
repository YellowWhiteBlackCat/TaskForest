use super::*;

#[test]
fn sensor_refresh_has_coherent_discovery_authority() {
    let snapshot = MacSensorProvider
        .refresh(1)
        .expect("sensor provider returns a typed snapshot");
    taskmanager_platform_conformance::assert_device_discovery_consistent(&snapshot)
        .expect("macOS sensor discovery must be coherent");
    if !snapshot.value.readings.is_empty() {
        assert_eq!(
            snapshot.discovery().outcome,
            taskmanager_core::SourceOutcome::Partial(FailureKind::Unsupported),
            "label-only component identity must stay explicitly degraded"
        );
    }
}
