use super::*;

#[test]
fn confirmed_empty_trip_set_is_distinct_from_unavailable() {
    let empty = ThermalTripPointSet::available(Vec::new(), 10);
    let denied = ThermalTripPointSet::unavailable(FailureKind::PermissionDenied);

    assert_eq!(empty.current_points(), Some([].as_slice()));
    assert_eq!(empty.last_success_ms, Some(10));
    assert_eq!(denied.current_points(), None);
    assert_eq!(
        denied.availability,
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
}

#[test]
fn cooling_activity_does_not_serialize_as_throttle_or_percentage() {
    let device = ThermalCoolingDeviceStatus {
        id: "cooling:fan:fixture:channel".into(),
        device_id: DeviceId::new("cooling:fan:fixture"),
        device_generation: DeviceGeneration::INITIAL,
        kind: ScalarObservation::available(ThermalCoolingKind::Fan, 10),
        current_state: ScalarObservation::available(1, 10),
        maximum_state: ScalarObservation::available(7, 10),
        activity: ScalarObservation::available(ThermalCoolingActivity::Active, 10),
    };

    let json = serde_json::to_string(&device).expect("cooling status serialization");
    assert!(json.contains("\"current_state\""));
    assert!(json.contains("\"maximum_state\""));
    assert!(!json.contains("usage_pct"));
    assert!(!json.contains("\"throttled\""));
}

#[test]
fn old_sensor_snapshot_defaults_the_new_control_sidecar() {
    let decoded: super::super::SensorCenterSnapshot = serde_json::from_str(
        r#"{
                "state":{"status":"healthy","last_success_ms":10},
                "timestamp_ms":10,
                "readings":[],
                "device_lifecycles":{}
            }"#,
    )
    .expect("pre-control sensor snapshot");

    assert!(decoded.thermal_control.zones.is_empty());
    assert!(decoded.thermal_control.cooling_devices.is_empty());
    assert_eq!(
        decoded
            .thermal_control
            .throttle
            .core_events_observation()
            .availability(),
        ScalarAvailability::Unknown
    );
}
