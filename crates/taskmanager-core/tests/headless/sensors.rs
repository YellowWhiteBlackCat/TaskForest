//! Sensor lifecycle and thermal-availability regression tests.

use super::*;

#[cfg(test)]
fn temperature_snapshot(
    timestamp_ms: u64,
    value: Option<f64>,
    state: DeviceState,
) -> SensorCenterSnapshot {
    SensorCenterSnapshot {
        state: DeviceState::healthy(timestamp_ms),
        timestamp_ms,
        readings: vec![temperature_reading(
            DeviceId::new("hwmon:pci:0000:00:01.0"),
            "hwmon:pci:0000:00:01.0:temp1",
            value,
            state,
        )],
        thermal_control: Default::default(),
        device_lifecycles: HashMap::new(),
    }
}

#[cfg(test)]
fn temperature_reading(
    device_id: DeviceId,
    id: &str,
    value: Option<f64>,
    state: DeviceState,
) -> SensorReading {
    let descriptor = SensorDescriptor::temperature(SensorScale::IDENTITY);
    let observation = match value {
        Some(value) if state.status == DeviceStatus::Healthy => {
            SensorMeasurementObservation::available(
                descriptor.clone(),
                SensorMagnitude::Decimal(value),
                state.last_success_ms.unwrap_or_default(),
            )
            .unwrap_or_else(|_| {
                SensorMeasurementObservation::unavailable(descriptor, FailureKind::ProviderFault)
            })
        }
        _ => SensorMeasurementObservation::unavailable(
            descriptor,
            state.status.failure().unwrap_or(FailureKind::ProviderFault),
        ),
    };
    SensorReading::from_measurement_observation(device_id, id.into(), "Package".into(), observation)
}

#[cfg(test)]
fn fan_reading(device_id: DeviceId, id: &str, rpm: u64, observed_at_ms: u64) -> SensorReading {
    let descriptor = SensorDescriptor::fan_speed(SensorScale::IDENTITY);
    let observation = SensorMeasurementObservation::available(
        descriptor.clone(),
        SensorMagnitude::Unsigned(rpm),
        observed_at_ms,
    )
    .unwrap_or_else(|_| {
        SensorMeasurementObservation::unavailable(descriptor, FailureKind::ProviderFault)
    });
    SensorReading::from_measurement_observation(device_id, id.into(), "Fan".into(), observation)
}

#[cfg(test)]
mod throttle_availability_tests {
    use super::*;
    use crate::core::FailureKind;

    #[test]
    fn typed_failure_retains_a_legacy_success_only_as_stale() {
        let previous = ThermalThrottleSnapshot::from_observations(
            10,
            ScalarObservation::available(0, 10),
            ScalarObservation::default(),
        );
        let current = ThermalThrottleSnapshot::from_observations(
            20,
            ScalarObservation::unavailable(FailureKind::PermissionDenied),
            ScalarObservation::default(),
        );

        let retained = current.retain_previous(previous);

        assert_eq!(
            retained.core_events_observation().availability(),
            ScalarAvailability::Stale(FailureKind::PermissionDenied)
        );
        assert_eq!(
            retained.core_events_observation().last_known_value(),
            Some(&0)
        );
        assert_eq!(
            retained.core_events_observation().last_success_ms(),
            Some(10)
        );
        assert_eq!(retained.current_core_events(), None);
    }

    #[test]
    fn explicit_unavailability_never_falls_back_to_a_legacy_number() {
        let snapshot = ThermalThrottleSnapshot::from_observations(
            10,
            ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            ScalarObservation::default(),
        );

        assert_eq!(snapshot.current_core_events(), None);
    }

    #[test]
    fn legacy_throttle_number_migrates_only_when_legacy_state_is_current() {
        let current: ThermalThrottleSnapshot = serde_json::from_value(serde_json::json!({
            "state": {"status": "healthy", "last_success_ms": 10},
            "timestamp_ms": 10,
            "core_events": 7,
            "package_events": null
        }))
        .expect("legacy current throttle snapshot");
        let unavailable: ThermalThrottleSnapshot = serde_json::from_value(serde_json::json!({
            "state": {"status": "permission_denied", "last_success_ms": null},
            "timestamp_ms": 20,
            "core_events": 7,
            "package_events": null
        }))
        .expect("legacy unavailable throttle snapshot");

        assert_eq!(current.current_core_events(), Some(7));
        assert_eq!(
            current.core_events_observation().last_success_ms(),
            Some(10)
        );
        assert_eq!(unavailable.current_core_events(), None);
        assert_eq!(
            unavailable.core_events_observation().last_known_value(),
            None
        );
    }

    #[test]
    fn typed_throttle_failure_wins_over_conflicting_legacy_number() {
        let snapshot: ThermalThrottleSnapshot = serde_json::from_value(serde_json::json!({
            "state": {"status": "healthy", "last_success_ms": 10},
            "timestamp_ms": 20,
            "core_events": 99,
            "package_events": null,
            "core_events_observation": {
                "value": null,
                "availability": {"status": "unavailable", "failure": "permission_denied"},
                "last_success_ms": null
            }
        }))
        .expect("conflicting throttle snapshot");

        assert_eq!(snapshot.current_core_events(), None);
        assert_eq!(
            snapshot.core_events_observation().availability(),
            ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
        );
        let encoded = serde_json::to_value(snapshot).expect("serialize canonical throttle");
        assert_eq!(encoded["core_events"], serde_json::Value::Null);
    }

    #[test]
    fn typed_only_throttle_wire_preserves_partial_availability() {
        let typed = ScalarObservation::partial(7, 20, FailureKind::TemporarilyUnavailable);
        let snapshot: ThermalThrottleSnapshot = serde_json::from_value(serde_json::json!({
            "timestamp_ms": 20,
            "core_events_observation": typed
        }))
        .expect("typed-only throttle snapshot");

        assert_eq!(snapshot.current_core_events(), Some(7));
        assert_eq!(
            snapshot.core_events_observation().availability(),
            ScalarAvailability::Partial(FailureKind::TemporarilyUnavailable)
        );
        assert_eq!(snapshot.state(), DeviceState::healthy(20));
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::core::FailureKind;
    use crate::core::device_state::DevicePresence;

    #[test]
    fn sensor_lifecycle_add_stale_absent_readd_keeps_unknown_explicit() {
        let mut tracker = SensorLifecycleTracker::new(100);
        let mut first = temperature_snapshot(10, Some(40.0), DeviceState::healthy(10));
        tracker.reconcile(&mut first);
        assert_eq!(
            tracker
                .lifecycle("hwmon:pci:0000:00:01.0")
                .map(|lifecycle| lifecycle.generation),
            Some(1)
        );
        assert_eq!(first.readings[0].device_generation().get(), 1);

        let mut stale = temperature_snapshot(
            20,
            None,
            DeviceState {
                status: DeviceStatus::Stale,
                last_success_ms: None,
            },
        );
        tracker.reconcile(&mut stale);
        assert_eq!(stale.readings[0].current_number(), None);
        assert_eq!(stale.readings[0].state().last_success_ms, Some(10));

        let mut absent = SensorCenterSnapshot {
            state: DeviceState::healthy(30),
            timestamp_ms: 30,
            readings: Vec::new(),
            thermal_control: Default::default(),
            device_lifecycles: HashMap::new(),
        };
        let removed = tracker.reconcile(&mut absent);
        assert_eq!(
            removed.newly_absent.first().map(|id| id.as_str()),
            Some("hwmon:pci:0000:00:01.0")
        );
        assert!(absent.readings.is_empty());
        assert_eq!(
            absent
                .device_lifecycles
                .get("hwmon:pci:0000:00:01.0")
                .map(|lifecycle| lifecycle.presence),
            Some(DevicePresence::Absent)
        );

        let mut readded = temperature_snapshot(40, Some(41.0), DeviceState::healthy(40));
        tracker.reconcile(&mut readded);
        assert_eq!(
            tracker
                .lifecycle("hwmon:pci:0000:00:01.0")
                .map(|lifecycle| lifecycle.generation),
            Some(2)
        );
        assert_eq!(readded.readings[0].device_generation().get(), 2);
    }

    #[test]
    fn sensor_enumeration_outage_is_unavailable_not_hot_unplug() {
        let mut tracker = SensorLifecycleTracker::new(100);
        let mut first = SensorCenterSnapshot {
            state: DeviceState::healthy(10),
            timestamp_ms: 10,
            readings: vec![fan_reading(
                DeviceId::new("hwmon:fixture"),
                "hwmon:fixture:fan1",
                900,
                10,
            )],
            thermal_control: Default::default(),
            device_lifecycles: HashMap::new(),
        };
        tracker.reconcile(&mut first);

        let mut denied = SensorCenterSnapshot {
            state: DeviceState {
                status: DeviceStatus::PermissionDenied,
                last_success_ms: None,
            },
            timestamp_ms: 20,
            readings: Vec::new(),
            thermal_control: Default::default(),
            device_lifecycles: HashMap::new(),
        };
        let delta = tracker.reconcile(&mut denied);
        assert!(delta.newly_absent.is_empty());
        let lifecycle = tracker
            .lifecycle("hwmon:fixture")
            .expect("provider outage retains sensor identity");
        assert_eq!(lifecycle.presence, DevicePresence::Unavailable);
        assert_eq!(lifecycle.state.last_success_ms, Some(10));
        assert_eq!(
            denied
                .device_lifecycles
                .get("hwmon:fixture")
                .map(|lifecycle| lifecycle.presence),
            Some(DevicePresence::Unavailable)
        );
    }

    #[test]
    fn retained_sensor_row_cannot_override_explicit_discovery_absence() {
        let mut tracker = SensorLifecycleTracker::new(100);
        let device_id = DeviceId::new("hwmon:fixture");
        let mut first = SensorCenterSnapshot {
            state: DeviceState::healthy(10),
            timestamp_ms: 10,
            readings: vec![fan_reading(
                device_id.clone(),
                "hwmon:fixture:fan1",
                900,
                10,
            )],
            thermal_control: Default::default(),
            device_lifecycles: HashMap::new(),
        };
        tracker.reconcile_discovered(
            &mut first,
            std::slice::from_ref(&device_id),
            DeviceRefreshOutcome::Complete,
        );

        let mut retained = first;
        retained.timestamp_ms = 20;
        let delta =
            tracker.reconcile_discovered(&mut retained, &[], DeviceRefreshOutcome::Complete);

        assert_eq!(delta.newly_absent, [device_id]);
        assert_eq!(retained.readings[0].state().status, DeviceStatus::Stale);
        assert_eq!(retained.readings[0].current_number(), None);
        assert_eq!(
            retained.readings[0]
                .measurement_observation()
                .availability(),
            ScalarAvailability::Stale(crate::core::FailureKind::TemporarilyUnavailable)
        );
        assert_eq!(
            retained
                .device_lifecycles
                .get("hwmon:fixture")
                .map(|lifecycle| lifecycle.presence),
            Some(DevicePresence::Absent)
        );
    }

    #[test]
    fn unavailable_refresh_never_prunes_unobserved_channels() {
        let mut tracker = SensorLifecycleTracker::new(100);
        let device_id = DeviceId::new("hwmon:pci:0000:00:01.0");
        let mut first = temperature_snapshot(10, Some(40.0), DeviceState::healthy(10));
        tracker.reconcile_discovered(
            &mut first,
            std::slice::from_ref(&device_id),
            DeviceRefreshOutcome::Complete,
        );
        let channel_id = first.readings[0].id().to_owned();

        let mut unavailable = temperature_snapshot(20, Some(41.0), DeviceState::healthy(20));
        unavailable.readings.clear();
        tracker.reconcile_discovered(
            &mut unavailable,
            &[],
            DeviceRefreshOutcome::Unavailable(DeviceStatus::Stale),
        );

        let mut failed = temperature_snapshot(
            30,
            None,
            DeviceState::default().transition(DeviceStatus::Stale, 30),
        );
        tracker.reconcile_discovered(
            &mut failed,
            std::slice::from_ref(&device_id),
            DeviceRefreshOutcome::Complete,
        );
        assert_eq!(failed.readings[0].id(), channel_id);
        assert_eq!(failed.readings[0].last_known_number(), Some(40.0));
    }

    #[test]
    fn reused_channel_id_takes_the_new_devices_observation() {
        let mut tracker = SensorLifecycleTracker::new(100);
        let dev_a = DeviceId::new("hwmon:dev-a");
        let mut first = temperature_snapshot(10, Some(40.0), DeviceState::healthy(10));
        first.readings[0] = temperature_reading(
            dev_a.clone(),
            "hwmon:pci:0000:00:01.0:temp1",
            Some(40.0),
            DeviceState::healthy(10),
        );
        tracker.reconcile_discovered(
            &mut first,
            std::slice::from_ref(&dev_a),
            DeviceRefreshOutcome::Complete,
        );
        let channel_id = first.readings[0].id().to_owned();

        let dev_b = DeviceId::new("hwmon:dev-b");
        let mut second = temperature_snapshot(20, Some(60.0), DeviceState::healthy(20));
        second.readings[0] = temperature_reading(
            dev_b.clone(),
            &channel_id,
            Some(60.0),
            DeviceState::healthy(20),
        );
        tracker.reconcile_discovered(
            &mut second,
            std::slice::from_ref(&dev_b),
            DeviceRefreshOutcome::Complete,
        );

        assert_eq!(
            second.readings[0].current_number(),
            Some(60.0),
            "the new device's value must win over the retained old observation"
        );
        assert_eq!(second.readings[0].device_id(), &dev_b);
    }

    #[test]
    fn reused_channel_id_never_echoes_the_old_devices_value_when_unavailable() {
        let mut tracker = SensorLifecycleTracker::new(100);
        let dev_a = DeviceId::new("hwmon:dev-a");
        let descriptor = SensorDescriptor::try_new(
            SensorQuantity::Temperature,
            SensorUnit::Celsius,
            Some(SensorScale::IDENTITY),
        )
        .expect("valid fixture descriptor");
        let mut first = SensorCenterSnapshot {
            state: DeviceState::healthy(10),
            timestamp_ms: 10,
            readings: vec![SensorReading::from_measurement_observation(
                dev_a.clone(),
                "slot:fan1".into(),
                "Package".into(),
                SensorMeasurementObservation::available(
                    descriptor.clone(),
                    SensorMagnitude::Decimal(40.0),
                    10,
                )
                .expect("valid fixture observation"),
            )],
            thermal_control: Default::default(),
            device_lifecycles: HashMap::new(),
        };
        tracker.reconcile_discovered(
            &mut first,
            std::slice::from_ref(&dev_a),
            DeviceRefreshOutcome::Complete,
        );

        let dev_b = DeviceId::new("hwmon:dev-b");
        let mut second = SensorCenterSnapshot {
            state: DeviceState::healthy(20),
            timestamp_ms: 20,
            readings: vec![SensorReading::from_measurement_observation(
                dev_b.clone(),
                first.readings[0].id().to_owned(),
                "Package".into(),
                SensorMeasurementObservation::unavailable(
                    descriptor,
                    FailureKind::TemporarilyUnavailable,
                ),
            )],
            thermal_control: Default::default(),
            device_lifecycles: HashMap::new(),
        };
        tracker.reconcile_discovered(
            &mut second,
            std::slice::from_ref(&dev_b),
            DeviceRefreshOutcome::Complete,
        );

        assert_eq!(
            second.readings[0].current_number(),
            None,
            "a different device's earlier value must not be echoed as current"
        );
        assert_eq!(
            second.readings[0]
                .measurement_observation()
                .last_known_value(),
            None,
            "a different device's earlier value must not be borrowed as last-known"
        );
    }

    #[test]
    fn channels_share_one_physical_generation_and_channel_churn_is_not_hotplug() {
        let mut tracker = SensorLifecycleTracker::new(100);
        let mut snapshot = temperature_snapshot(10, Some(40.0), DeviceState::healthy(10));
        snapshot.readings.push(fan_reading(
            DeviceId::new("hwmon:pci:0000:00:01.0"),
            "hwmon:pci:0000:00:01.0:fan1",
            900,
            10,
        ));
        tracker.reconcile(&mut snapshot);
        assert!(
            snapshot
                .readings
                .iter()
                .all(|reading| reading.device_generation().get() == 1)
        );
        assert_eq!(snapshot.device_lifecycles.len(), 1);

        snapshot.timestamp_ms = 20;
        snapshot.readings.pop();
        let delta = tracker.reconcile(&mut snapshot);
        assert!(delta.newly_absent.is_empty());
        assert_eq!(snapshot.readings[0].device_generation().get(), 1);
    }
}

#[cfg(test)]
mod sensor_state_aggregation_tests {
    use super::*;

    fn state(status: DeviceStatus, last_success_ms: Option<u64>) -> DeviceState {
        DeviceState {
            status,
            last_success_ms,
        }
    }

    #[test]
    fn higher_priority_status_wins_regardless_of_order() {
        for (current, observed) in [
            (DeviceStatus::Healthy, DeviceStatus::Stale),
            (DeviceStatus::Stale, DeviceStatus::MissingTool),
            (DeviceStatus::MissingTool, DeviceStatus::PermissionDenied),
        ] {
            assert_eq!(
                aggregate_sensor_state(state(current, Some(1)), state(observed, Some(2))).status,
                observed,
                "{observed:?} outranks {current:?}"
            );
        }
        assert_eq!(
            aggregate_sensor_state(
                state(DeviceStatus::PermissionDenied, Some(1)),
                state(DeviceStatus::Stale, Some(2))
            )
            .status,
            DeviceStatus::PermissionDenied
        );
    }

    #[test]
    fn equal_priority_keeps_the_current_status() {
        assert_eq!(
            aggregate_sensor_state(
                state(DeviceStatus::Healthy, Some(5)),
                state(DeviceStatus::Healthy, Some(9))
            )
            .status,
            DeviceStatus::Healthy
        );
    }

    #[test]
    fn unsupported_dominates_healthy_regardless_of_order() {
        // Unavailable dominates healthy: a device with healthy and
        // unsupported channels must roll up unsupported so the gap stays
        // visible instead of being masked by the healthy channel.
        assert_eq!(
            aggregate_sensor_state(
                state(DeviceStatus::Unsupported, None),
                state(DeviceStatus::Healthy, Some(3))
            )
            .status,
            DeviceStatus::Unsupported
        );
        assert_eq!(
            aggregate_sensor_state(
                state(DeviceStatus::Healthy, Some(3)),
                state(DeviceStatus::Unsupported, None)
            )
            .status,
            DeviceStatus::Unsupported
        );
    }

    #[test]
    fn last_success_merges_to_the_latest_observation() {
        assert_eq!(
            aggregate_sensor_state(
                state(DeviceStatus::Stale, Some(5)),
                state(DeviceStatus::Stale, Some(9))
            )
            .last_success_ms,
            Some(9)
        );
        assert_eq!(
            aggregate_sensor_state(
                state(DeviceStatus::Stale, Some(5)),
                state(DeviceStatus::Healthy, None)
            )
            .last_success_ms,
            Some(5)
        );
        assert_eq!(
            aggregate_sensor_state(
                state(DeviceStatus::Stale, None),
                state(DeviceStatus::Healthy, None)
            )
            .last_success_ms,
            None
        );
    }
}
