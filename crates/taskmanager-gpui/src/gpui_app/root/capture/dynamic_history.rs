//! Controlled battery samples enter the normal correlated history ingestor.
use super::fixtures::dynamic_power_fixture;
use taskmanager_core::core::{BatteryScalarObservations, DeviceId, DeviceState, ScalarObservation};
use taskmanager_telemetry_store::{
    CorrelatedSystemTelemetryHistory, CorrelatedSystemTelemetryIngestor, CorrelatedTelemetryStamp,
};

pub(super) fn seed(
    history: &CorrelatedSystemTelemetryHistory,
    ingestor: &CorrelatedSystemTelemetryIngestor,
    anchor: u64,
) -> bool {
    for index in 0..8_u8 {
        let timestamp = anchor.max(1).saturating_add(u64::from(index) + 1);
        let Some(stamp) = CorrelatedTelemetryStamp::from_accepted_event(
            u64::MAX - 8 + u64::from(index),
            timestamp,
        ) else {
            return false;
        };
        let mut power = dynamic_power_fixture();
        power.timestamp_ms = timestamp;
        power.state = DeviceState::healthy(timestamp);
        let Some(battery) = power.batteries.first_mut() else {
            return false;
        };
        battery.device_state = DeviceState::healthy(timestamp);
        battery.apply_scalar_observations(BatteryScalarObservations {
            capacity_pct: ScalarObservation::available(85 - index, timestamp),
            voltage_uv: ScalarObservation::available(12_100_000, timestamp),
            power_w: ScalarObservation::available(12.8 + f32::from(index) * 0.2, timestamp),
            cycle_count: ScalarObservation::available(142, timestamp),
            ..Default::default()
        });
        if ingestor
            .ingest_correlated_power_supplies(stamp, &power)
            .is_err()
        {
            return false;
        }
    }
    let dynamic = history.dynamic_history();
    let id = DeviceId::new("power-supply:capture-battery");
    dynamic
        .battery_capacity_pct(&id)
        .is_some_and(|ring| ring.samples().len() >= 8)
        && dynamic
            .battery_power_w(&id)
            .is_some_and(|ring| ring.samples().len() >= 8)
}
