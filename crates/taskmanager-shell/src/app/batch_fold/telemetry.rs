//! System telemetry, dynamic-device and alert-watermark fold systems.

use std::collections::BTreeSet;

use super::super::device_rows::DeviceRowFamily;
use super::*;
use taskmanager_application::{
    CorrelatedPowerSupplyEvent, CorrelatedSensorEvent, CorrelatedSystemTelemetryOutcome,
    DeviceLifecycleApplyResult, DeviceLifecyclePartition, DeviceLifecycleProjection,
    DeviceLifecycleSnapshotRevision,
};

pub(super) fn apply_system_telemetry(
    store: &mut SystemProjectionStore,
    outcomes: &[CorrelatedSystemTelemetryOutcome],
    projections: Vec<ProjectedSystemTelemetry>,
    fold: &mut FoldState,
) {
    let received = !outcomes.is_empty() || !projections.is_empty();
    if !received {
        return;
    }

    fold.mark_updated();
    fold.output.changes.telemetry = true;
    let mut committed_snapshot = false;
    let mut accepted_families = BTreeSet::new();
    for correlated in outcomes {
        let accepted_partition = system_telemetry::apply_system_outcome_lifecycle(
            &mut store.device_lifecycle_projection,
            &mut store.device_lifecycle_diagnostics,
            correlated,
        );
        if let Some(partition) = accepted_partition {
            let family = match partition {
                DeviceLifecyclePartition::SystemStorage => Some(DeviceRowFamily::Storage),
                DeviceLifecyclePartition::SystemNetwork => Some(DeviceRowFamily::Network),
                DeviceLifecyclePartition::SystemGpu => Some(DeviceRowFamily::Gpu),
                DeviceLifecyclePartition::Sensors
                | DeviceLifecyclePartition::PowerSupplies
                | DeviceLifecyclePartition::NpuInventory => None,
            };
            if let Some(family) = family {
                store.queue_device_row_family_change(family);
                accepted_families.insert(family);
            }
        }
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
        committed_snapshot |= matches!(applied, system_telemetry::SystemTelemetryApply::Committed);
    }
    if committed_snapshot {
        store.commit_device_row_family_changes();
    } else {
        for family in accepted_families {
            store.note_device_row_family_changed(family);
        }
    }
}

pub(super) fn apply_dynamic_devices(
    store: &mut SystemProjectionStore,
    sensor_events: Vec<CorrelatedSensorEvent>,
    power_events: Vec<CorrelatedPowerSupplyEvent>,
    fold: &mut FoldState,
) {
    for correlated in sensor_events {
        let SensorEvent::Snapshot(snapshot) = correlated.event;
        let (sensors, _discovered_devices, sources) = snapshot.into_value_and_sources();
        let lifecycle_result = store.device_lifecycle_projection.apply_sensor_snapshot(
            DeviceLifecycleSnapshotRevision::new(correlated.sequence.get()),
            &sensors,
        );
        let accepted = matches!(&lifecycle_result, DeviceLifecycleApplyResult::Applied(_));
        store.device_lifecycle_diagnostics.record(lifecycle_result);
        if !accepted {
            continue;
        }
        store.sensors = Some(sensors);
        store.sensor_source = Some(sources);
        store.sensor_stamp = Some(DynamicDeviceProjectionStamp {
            sequence: correlated.sequence.get(),
            observed_at_ms: correlated.observed_at_ms,
        });
        store.note_device_row_family_changed(DeviceRowFamily::Sensor);
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
        let accepted = matches!(&lifecycle_result, DeviceLifecycleApplyResult::Applied(_));
        store.device_lifecycle_diagnostics.record(lifecycle_result);
        if !accepted {
            continue;
        }
        store.power_supplies = Some(power_supplies);
        store.power_supply_source = Some(sources);
        store.power_supply_stamp = Some(DynamicDeviceProjectionStamp {
            sequence: correlated.sequence.get(),
            observed_at_ms: correlated.observed_at_ms,
        });
        store.note_device_row_family_changed(DeviceRowFamily::PowerSupply);
        fold.output.changes.dynamic_devices = true;
        fold.mark_updated();
    }
}

impl SystemProjectionStore {
    pub(crate) fn seed_sensor_snapshot(&mut self, snapshot: Option<SensorCenterSnapshot>) {
        let Some(sensors) = snapshot else {
            self.sensors = None;
            self.note_device_row_family_changed(DeviceRowFamily::Sensor);
            return;
        };
        let revision = next_fixture_lifecycle_revision(
            &self.device_lifecycle_projection,
            DeviceLifecyclePartition::Sensors,
        );
        let result = self
            .device_lifecycle_projection
            .apply_sensor_snapshot(revision, &sensors);
        let accepted = matches!(&result, DeviceLifecycleApplyResult::Applied(_));
        self.device_lifecycle_diagnostics.record(result);
        if accepted {
            self.sensors = Some(sensors);
            self.note_device_row_family_changed(DeviceRowFamily::Sensor);
        }
    }

    pub(crate) fn seed_power_supply_snapshot(&mut self, snapshot: Option<PowerSupplySnapshot>) {
        let Some(power_supplies) = snapshot else {
            self.power_supplies = None;
            self.note_device_row_family_changed(DeviceRowFamily::PowerSupply);
            return;
        };
        let revision = next_fixture_lifecycle_revision(
            &self.device_lifecycle_projection,
            DeviceLifecyclePartition::PowerSupplies,
        );
        let result = self
            .device_lifecycle_projection
            .apply_power_supply_snapshot(revision, &power_supplies);
        let accepted = matches!(&result, DeviceLifecycleApplyResult::Applied(_));
        self.device_lifecycle_diagnostics.record(result);
        if accepted {
            self.power_supplies = Some(power_supplies);
            self.note_device_row_family_changed(DeviceRowFamily::PowerSupply);
        }
    }
}

fn next_fixture_lifecycle_revision(
    projection: &DeviceLifecycleProjection,
    partition: DeviceLifecyclePartition,
) -> DeviceLifecycleSnapshotRevision {
    let next = projection
        .accepted_revision_for(partition)
        .map_or(1, |revision| revision.get().saturating_add(1));
    DeviceLifecycleSnapshotRevision::new(next)
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
    store.record_alert_projection_changes(evaluation.changes);
    for request in evaluation.notifications {
        if store.pending_notifications.len() < MAX_PENDING_NOTIFICATIONS {
            store.pending_notifications.push_back(request);
        }
    }
    fold.output.changes.snapshot_recorded = true;
}
