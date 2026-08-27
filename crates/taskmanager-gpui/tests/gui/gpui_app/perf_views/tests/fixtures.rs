//! Typed dynamic-device fixtures shared by Performance render tests.

use super::*;

pub(super) fn sensor_reading(
    device_id: DeviceId,
    id: &str,
    label: &str,
    descriptor: SensorDescriptor,
    magnitude: SensorMagnitude,
    observed_at_ms: u64,
    generation: u64,
) -> SensorReading {
    SensorReading::from_measurement_observation(
        device_id,
        id.into(),
        label.into(),
        SensorMeasurementObservation::available(descriptor, magnitude, observed_at_ms)
            .expect("valid sensor fixture"),
    )
    .with_device_generation(DeviceGeneration::new(generation))
}

pub(super) fn with_battery_scalars(
    mut battery: BatteryInfo,
    observed_at_ms: u64,
    capacity_pct: u8,
    power_w: f32,
) -> BatteryInfo {
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(capacity_pct, observed_at_ms),
        power_w: ScalarObservation::available(power_w, observed_at_ms),
        ..Default::default()
    });
    battery
}
