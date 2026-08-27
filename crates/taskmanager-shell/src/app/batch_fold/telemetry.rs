//! System telemetry, dynamic-device and alert-watermark fold systems.

use super::*;

pub(super) fn apply_system_telemetry(
    store: &mut SystemProjectionStore,
    outcomes: &[taskmanager_application::CorrelatedSystemTelemetryOutcome],
    projections: Vec<ProjectedSystemTelemetry>,
    fold: &mut FoldState,
) {
    let received = !outcomes.is_empty() || !projections.is_empty();
    if !received {
        return;
    }

    fold.mark_updated();
    fold.output.changes.telemetry = true;
    for correlated in outcomes {
        system_telemetry::apply_system_outcome_lifecycle(
            &mut store.device_lifecycle_projection,
            &mut store.device_lifecycle_diagnostics,
            correlated,
        );
    }
    for projection in projections {
        let applied = system_telemetry::apply_projected_system_telemetry(
            &mut store.system_telemetry,
            &mut store.snapshot,
            projection,
        );
        debug_assert!(applied.is_accepted() || !applied.frame_commit().is_committed());
        fold.output.changes.frame_commit = fold
            .output
            .changes
            .frame_commit
            .merge(applied.frame_commit());
    }
}

pub(super) fn apply_dynamic_devices(
    store: &mut SystemProjectionStore,
    sensor_events: Vec<taskmanager_application::CorrelatedSensorEvent>,
    power_events: Vec<taskmanager_application::CorrelatedPowerSupplyEvent>,
    fold: &mut FoldState,
) {
    for correlated in sensor_events {
        let SensorEvent::Snapshot(snapshot) = correlated.event;
        let (sensors, _discovered_devices, sources) = snapshot.into_value_and_sources();
        let lifecycle_result = store.device_lifecycle_projection.apply_sensor_snapshot(
            DeviceLifecycleSnapshotRevision::new(correlated.sequence.get()),
            &sensors,
        );
        store.device_lifecycle_diagnostics.record(lifecycle_result);
        store.sensors = Some(sensors);
        store.sensor_source = Some(sources);
        store.sensor_stamp = Some(DynamicDeviceProjectionStamp {
            sequence: correlated.sequence.get(),
            observed_at_ms: correlated.observed_at_ms,
        });
        fold.output.changes.dynamic_devices = true;
        fold.mark_updated();
    }
    for correlated in power_events {
        let PowerSupplyEvent::Snapshot(snapshot) = correlated.event;
        let (power_supplies, _discovered_devices, sources) = snapshot.into_value_and_sources();
        let lifecycle_result = store
            .device_lifecycle_projection
            .apply_power_supply_snapshot(
                DeviceLifecycleSnapshotRevision::new(correlated.sequence.get()),
                &power_supplies,
            );
        store.device_lifecycle_diagnostics.record(lifecycle_result);
        store.power_supplies = Some(power_supplies);
        store.power_supply_source = Some(sources);
        store.power_supply_stamp = Some(DynamicDeviceProjectionStamp {
            sequence: correlated.sequence.get(),
            observed_at_ms: correlated.observed_at_ms,
        });
        fold.output.changes.dynamic_devices = true;
        fold.mark_updated();
    }
}

pub(super) fn evaluate_new_snapshot(store: &mut SystemProjectionStore, fold: &mut FoldState) {
    let Some(snapshot) = store.snapshot.as_ref() else {
        return;
    };
    if snapshot.timestamp_ms == store.last_recorded_snapshot_ms {
        return;
    }

    store.last_recorded_snapshot_ms = snapshot.timestamp_ms;
    let evaluation: AlertEvaluation = store.alert_center.evaluate(snapshot, submission_time_ms());
    store.alert_active = evaluation.active;
    for request in evaluation.notifications {
        if store.pending_notifications.len() < MAX_PENDING_NOTIFICATIONS {
            store.pending_notifications.push_back(request);
        }
    }
    fold.output.changes.snapshot_recorded = true;
}
