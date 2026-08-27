use super::*;
use crate::core::{
    DevicePresence, DeviceRefreshOutcome, SensorCenterSnapshot, SensorLifecycleTracker,
};

fn identity() -> DeviceId {
    DeviceId::new("hwmon:fixture")
}

fn reading(observation: ScalarObservation<SensorValue>) -> SensorReading {
    SensorReading::from_measurement_observation(
        identity(),
        "hwmon:fixture:fan1".into(),
        "Fan".into(),
        measurement_from_legacy(SensorKind::Fan, observation),
    )
}

fn voltage_reading(observation: SensorMeasurementObservation) -> SensorReading {
    SensorReading::from_measurement_observation(
        identity(),
        "hwmon:fixture:in0".into(),
        "Voltage".into(),
        observation,
    )
}

#[test]
fn real_zero_is_current_and_timestamped() {
    let reading = reading(ScalarObservation::available(SensorValue::FanRpm(0), 42));

    assert_eq!(reading.current_number(), Some(0.0));
    assert_eq!(
        reading.measurement_observation().availability(),
        ScalarAvailability::Available
    );
    assert_eq!(
        reading.measurement_observation().last_success_ms(),
        Some(42)
    );
}

#[test]
fn partial_reading_keeps_current_value_and_typed_failure() {
    let reading = reading(ScalarObservation::partial(
        SensorValue::FanRpm(900),
        42,
        FailureKind::TemporarilyUnavailable,
    ));

    assert_eq!(reading.current_number(), Some(900.0));
    assert_eq!(
        reading.measurement_observation().availability(),
        ScalarAvailability::Partial(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        reading.measurement_observation().last_success_ms(),
        Some(42)
    );
}

#[test]
fn typed_failures_remain_distinct_when_coarse_device_state_cannot() {
    for failure in [
        FailureKind::Unsupported,
        FailureKind::PermissionDenied,
        FailureKind::TemporarilyUnavailable,
        FailureKind::ProviderFault,
    ] {
        let reading = reading(ScalarObservation::unavailable(failure));
        assert_eq!(
            reading.measurement_observation().availability(),
            ScalarAvailability::Unavailable(failure)
        );
        assert_eq!(reading.current_number(), None);
    }
}

#[test]
fn legacy_only_wire_migrates_to_typed_measurement() {
    let decoded: SensorReading = serde_json::from_value(serde_json::json!({
        "device_id": "hwmon:fixture",
        "device_generation": 0,
        "id": "hwmon:fixture:fan1",
        "label": "Fan",
        "kind": "fan",
        "value": {"fan_rpm": 900},
        "state": {"status": "healthy", "last_success_ms": 10}
    }))
    .expect("legacy sensor reading");

    assert_eq!(decoded.quantity(), &SensorQuantity::FanSpeed);
    assert_eq!(decoded.current_number(), Some(900.0));
    assert_eq!(
        decoded.measurement_observation().last_success_ms(),
        Some(10)
    );
}

#[test]
fn typed_measurement_wins_over_conflicting_legacy_fields() {
    let typed = SensorMeasurementObservation::available(
        SensorDescriptor::temperature(SensorScale::IDENTITY),
        SensorMagnitude::Decimal(42.5),
        20,
    )
    .expect("typed fixture");
    let decoded: SensorReading = serde_json::from_value(serde_json::json!({
        "device_id": "hwmon:fixture",
        "device_generation": 0,
        "id": "hwmon:fixture:temp1",
        "label": "Package",
        "kind": "fan",
        "value": {"fan_rpm": 900},
        "measurement_observation": typed,
        "state": {"status": "healthy", "last_success_ms": 10}
    }))
    .expect("conflicting sensor reading");

    assert_eq!(decoded.quantity(), &SensorQuantity::Temperature);
    assert_eq!(decoded.current_number(), Some(42.5));
    assert_eq!(
        decoded.measurement_observation().last_success_ms(),
        Some(20)
    );
}

#[test]
fn typed_only_wire_deserializes_without_legacy_projection_fields() {
    let typed = SensorMeasurementObservation::partial(
        SensorDescriptor::temperature(SensorScale::IDENTITY),
        SensorMagnitude::Decimal(42.5),
        20,
        FailureKind::TemporarilyUnavailable,
    )
    .expect("typed fixture");
    let decoded: SensorReading = serde_json::from_value(serde_json::json!({
        "device_id": "hwmon:fixture",
        "device_generation": 0,
        "id": "hwmon:fixture:temp1",
        "label": "Package",
        "measurement_observation": typed
    }))
    .expect("typed-only sensor reading");

    assert_eq!(decoded.quantity(), &SensorQuantity::Temperature);
    assert_eq!(decoded.current_number(), Some(42.5));
    assert_eq!(
        decoded.measurement_observation().availability(),
        ScalarAvailability::Partial(FailureKind::TemporarilyUnavailable)
    );
}

#[test]
fn unknown_legacy_kind_never_fabricates_a_measurement() {
    let decoded: SensorReading = serde_json::from_value(serde_json::json!({
        "device_id": "hwmon:fixture",
        "device_generation": 0,
        "id": "hwmon:fixture:mystery",
        "label": "Mystery",
        "kind": "future_kind",
        "value": {"fan_rpm": 900},
        "state": {"status": "healthy", "last_success_ms": 10}
    }))
    .expect("unknown legacy kind");

    assert_eq!(decoded.quantity(), &SensorQuantity::Unknown);
    assert_eq!(decoded.current_measurement(), None);
}

#[test]
fn failed_channel_retains_last_value_as_stale_without_removing_device() {
    let mut tracker = SensorLifecycleTracker::new(100);
    let mut first = SensorCenterSnapshot {
        state: DeviceState::healthy(10),
        timestamp_ms: 10,
        readings: vec![reading(ScalarObservation::available(
            SensorValue::FanRpm(900),
            10,
        ))],
        ..SensorCenterSnapshot::default()
    };
    tracker.reconcile_discovered(&mut first, &[identity()], DeviceRefreshOutcome::Complete);

    let mut failed = SensorCenterSnapshot {
        state: DeviceState::healthy(20),
        timestamp_ms: 20,
        readings: vec![reading(ScalarObservation::unavailable(
            FailureKind::ProviderFault,
        ))],
        ..SensorCenterSnapshot::default()
    };
    tracker.reconcile_discovered(&mut failed, &[identity()], DeviceRefreshOutcome::Complete);

    assert_eq!(
        failed.readings[0].measurement_observation().availability(),
        ScalarAvailability::Stale(FailureKind::ProviderFault)
    );
    assert_eq!(failed.readings[0].current_number(), None);
    assert_eq!(failed.readings[0].last_known_number(), Some(900.0));
    assert_eq!(
        tracker
            .lifecycle(identity().as_str())
            .map(|lifecycle| lifecycle.presence),
        Some(DevicePresence::Present)
    );
}

#[test]
fn lifecycle_retains_nonlegacy_quantity_only_for_same_descriptor() {
    let mut tracker = SensorLifecycleTracker::new(100);
    let descriptor = SensorDescriptor::voltage(SensorScale::MILLI);
    let mut first = SensorCenterSnapshot {
        state: DeviceState::healthy(10),
        timestamp_ms: 10,
        readings: vec![voltage_reading(
            SensorMeasurementObservation::available(
                descriptor.clone(),
                SensorMagnitude::Signed(12_000),
                10,
            )
            .expect("valid voltage"),
        )],
        ..SensorCenterSnapshot::default()
    };
    tracker.reconcile_discovered(&mut first, &[identity()], DeviceRefreshOutcome::Complete);

    let mut failed = SensorCenterSnapshot {
        state: DeviceState::healthy(20),
        timestamp_ms: 20,
        readings: vec![voltage_reading(SensorMeasurementObservation::unavailable(
            descriptor,
            FailureKind::TemporarilyUnavailable,
        ))],
        ..SensorCenterSnapshot::default()
    };
    tracker.reconcile_discovered(&mut failed, &[identity()], DeviceRefreshOutcome::Complete);
    assert_eq!(
        failed.readings[0].measurement_observation().availability(),
        ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        failed.readings[0].last_known_measurement(),
        Some(SensorMagnitude::Signed(12_000))
    );

    let mut changed_descriptor = SensorCenterSnapshot {
        state: DeviceState::healthy(30),
        timestamp_ms: 30,
        readings: vec![voltage_reading(SensorMeasurementObservation::unavailable(
            SensorDescriptor::current(SensorScale::MILLI),
            FailureKind::ProviderFault,
        ))],
        ..SensorCenterSnapshot::default()
    };
    tracker.reconcile_discovered(
        &mut changed_descriptor,
        &[identity()],
        DeviceRefreshOutcome::Complete,
    );
    assert_eq!(
        changed_descriptor.readings[0]
            .measurement_observation()
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(
        changed_descriptor.readings[0].last_known_measurement(),
        None
    );
}
