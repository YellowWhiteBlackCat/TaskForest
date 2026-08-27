use super::*;

#[test]
fn live_win_sensor_provider_refresh() {
    let mut provider = WinSensorProvider::new();
    let result = provider.refresh(1000);
    assert!(result.is_ok());
    let snap = result.unwrap();
    taskmanager_platform_conformance::assert_device_discovery_consistent(&snap)
        .expect("Windows sensor discovery must be coherent");
    eprintln!(
        "LIVE WIN SENSOR TELEMETRY: status={:?}, readings count={}",
        snap.discovery().outcome,
        snap.value.readings.len()
    );
    for r in &snap.value.readings {
        eprintln!(
            "  SENSOR: id={}, label={}, value={:?}",
            r.id(),
            r.label(),
            r.current_measurement()
        );
    }
}
