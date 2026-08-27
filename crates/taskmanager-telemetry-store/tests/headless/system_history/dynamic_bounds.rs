use super::*;

fn partial_state(observed_at_ms: u64) -> DeviceState {
    DeviceState {
        status: DeviceStatus::Stale,
        last_success_ms: Some(observed_at_ms),
    }
}

fn battery(id: String, observed_at_ms: u64) -> BatteryInfo {
    let mut battery = BatteryInfo::new(id, DeviceState::healthy(observed_at_ms));
    battery.device_generation = DeviceGeneration::INITIAL;
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(50, observed_at_ms),
        power_w: ScalarObservation::available(5.0, observed_at_ms),
        ..Default::default()
    });
    battery
}

#[test]
fn partial_battery_identity_churn_is_bounded_and_gaps_retained_histories() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    let attempted = MAX_DYNAMIC_HISTORY_IDENTITIES + 24;
    let mut rejected = 0_usize;

    for index in 0..attempted {
        let revision = index as u64 + 1;
        let observed_at_ms = revision.saturating_mul(10);
        let report = ingestor
            .ingest_correlated_power_supplies(
                stamp_at(revision, observed_at_ms),
                &PowerSupplySnapshot {
                    state: partial_state(observed_at_ms),
                    timestamp_ms: observed_at_ms,
                    batteries: vec![battery(
                        format!("partial-battery-{index:04}"),
                        observed_at_ms,
                    )],
                    ..Default::default()
                },
            )
            .expect("monotonic partial battery snapshot");
        rejected = rejected.saturating_add(report.rejected_identity_capacity);
    }

    let retained = (0..attempted)
        .filter(|index| {
            store
                .dynamic_history
                .battery_capacity_pct(&DeviceId::new(format!("partial-battery-{index:04}")))
                .is_some()
        })
        .count();
    assert_eq!(retained, MAX_DYNAMIC_HISTORY_IDENTITIES);
    assert_eq!(rejected, attempted - MAX_DYNAMIC_HISTORY_IDENTITIES);
    assert!(
        store
            .dynamic_history
            .battery_power_w(&DeviceId::new(format!(
                "partial-battery-{:04}",
                MAX_DYNAMIC_HISTORY_IDENTITIES - 1
            )))
            .is_some(),
        "all battery scalar families must share the same admitted identities"
    );
    let oldest = store
        .dynamic_history
        .battery_capacity_pct(&DeviceId::new("partial-battery-0000"))
        .expect("the oldest partial identity remains retained");
    assert_eq!(
        oldest.samples().last().and_then(|sample| sample.value),
        None,
        "a retained identity missing from a later partial snapshot advances with an honest gap"
    );
}

fn fan(id: String, observed_at_ms: u64) -> SensorReading {
    fan_reading(
        DeviceId::new(format!("device:{id}")),
        id,
        "fan".to_owned(),
        1_200,
        observed_at_ms,
    )
    .with_device_generation(DeviceGeneration::INITIAL)
}

#[test]
fn partial_sensor_identity_churn_is_bounded_across_related_scalar_families() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(4);
    let attempted = MAX_DYNAMIC_HISTORY_IDENTITIES + 24;
    let mut rejected = 0_usize;

    for index in 0..attempted {
        let revision = index as u64 + 1;
        let observed_at_ms = revision.saturating_mul(10);
        let id = format!("partial-fan-{index:04}");
        let report = ingestor
            .ingest_correlated_sensors(
                stamp_at(revision, observed_at_ms),
                &SensorCenterSnapshot {
                    state: partial_state(observed_at_ms),
                    timestamp_ms: observed_at_ms,
                    readings: vec![fan(id, observed_at_ms)],
                    ..Default::default()
                },
            )
            .expect("monotonic partial sensor snapshot");
        rejected = rejected.saturating_add(report.rejected_identity_capacity);
    }

    let retained = (0..attempted)
        .filter(|index| {
            store
                .dynamic_history
                .fan_rpm(&DeviceId::new(format!("partial-fan-{index:04}")))
                .is_some()
        })
        .count();
    assert_eq!(retained, MAX_DYNAMIC_HISTORY_IDENTITIES);
    assert_eq!(rejected, attempted - MAX_DYNAMIC_HISTORY_IDENTITIES);
    assert!(
        store
            .dynamic_history
            .fan_pwm_pct(&DeviceId::new(format!(
                "partial-fan-{:04}",
                MAX_DYNAMIC_HISTORY_IDENTITIES - 1
            )))
            .is_some(),
        "RPM and PWM rings must share the same admitted fan identities"
    );
    let oldest = store
        .dynamic_history
        .fan_rpm(&DeviceId::new("partial-fan-0000"))
        .expect("the oldest partial fan remains retained");
    assert_eq!(
        oldest.samples().last().and_then(|sample| sample.value),
        None,
        "partial fan churn must append gaps instead of connecting measurements"
    );
}
