use super::*;
use taskforest_core::core::ScalarObservation;
use taskforest_core::core::device_state::DeviceStatus;
use taskforest_core::core::identity::DeviceGeneration;
use taskforest_core::core::identity::DeviceId;
use taskforest_core::core::power::BatteryScalarObservations;
use taskforest_core::core::sensors::{
    SensorDescriptor, SensorMagnitude, SensorMeasurementObservation, SensorScale,
};

#[test]
fn battery_and_fan_captions_format_reliably() {
    let mut battery = BatteryInfo::default();
    battery.id = "BAT0".to_string();
    battery.device_state.status = DeviceStatus::Healthy;
    battery.status = "Discharging".to_string();
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(84, 100),
        ..Default::default()
    });

    let (cap1, cap2) = battery_caption(&battery);
    assert_eq!(cap1, "84%");
    assert_eq!(cap2, "Discharging");

    let mut degraded_battery = battery;
    degraded_battery.device_state.status = DeviceStatus::PermissionDenied;
    let (_, cap2_degraded) = battery_caption(&degraded_battery);
    assert!(cap2_degraded.contains("Discharging"));

    let fan = SensorReading::from_measurement_observation(
        DeviceId::new("sensor:fan1"),
        "fan1".into(),
        "Chassis Fan 1".into(),
        SensorMeasurementObservation::available(
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(1200),
            1_000,
        )
        .expect("valid fan fixture"),
    )
    .with_device_generation(DeviceGeneration::new(1));
    let (rpm, label) = fan_caption(&fan);
    assert_eq!(rpm, "1200 RPM");
    assert_eq!(
        label, "Chassis Fan 1",
        "a healthy device adds no status badge"
    );
}
