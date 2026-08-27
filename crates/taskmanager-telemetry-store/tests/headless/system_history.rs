use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use taskmanager_core::{
    BatteryInfo, BatteryScalarObservations, CpuMetrics, CpuScalarObservations,
    CpuTelemetryObservation, DeviceGeneration, DeviceId, DeviceLifecycle, DevicePresence,
    DeviceState, DeviceStatus, DiskMetrics, FailureKind, GpuEngine, GpuEngineKind, GpuMetrics,
    GpuScalarObservations, GpuTelemetryObservation, HostRuntimeFacts, HostRuntimeObservation,
    MemoryMetrics, MemoryScalarObservations, MemoryTelemetryObservation, NetworkScalarObservations,
    NetworkTelemetryObservation, PowerSupplySnapshot, ScalarObservation, ScalarObservationGroup,
    ScalarObservationSlot, SensorCenterSnapshot, SensorDescriptor, SensorMagnitude,
    SensorMeasurementObservation, SensorReading, SensorScale, StorageTelemetryObservation,
};

use super::*;
use crate::TelemetryStore;

fn stamp(revision: u64) -> CorrelatedTelemetryStamp {
    CorrelatedTelemetryStamp::from_accepted_event(revision, revision.saturating_mul(10))
        .expect("test revisions are non-zero")
}

fn stamp_at(revision: u64, completed_at_ms: u64) -> CorrelatedTelemetryStamp {
    CorrelatedTelemetryStamp::from_accepted_event(revision, completed_at_ms)
        .expect("test revisions are non-zero")
}

fn available_group<T>(
    values: impl IntoIterator<Item = T>,
    at_ms: u64,
) -> ScalarObservationGroup<T> {
    ScalarObservationGroup::available(values.into_iter().collect(), at_ms)
}

fn observed_cpu(usage: f32, at_ms: u64) -> CpuMetrics {
    CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(usage, at_ms),
        ..Default::default()
    })
}

fn observed_memory(
    total: u64,
    used: u64,
    swap_total: u64,
    swap_used: u64,
    at_ms: u64,
) -> MemoryMetrics {
    MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(total, at_ms),
            used_bytes: ScalarObservation::available(used, at_ms),
            swap_total_bytes: ScalarObservation::available(swap_total, at_ms),
            swap_used_bytes: ScalarObservation::available(swap_used, at_ms),
            ..Default::default()
        },
        Default::default(),
    )
}

fn lifecycle(presence: DevicePresence, generation: u64, observed_at_ms: u64) -> DeviceLifecycle {
    DeviceLifecycle {
        presence,
        state: DeviceState::healthy(observed_at_ms),
        generation,
        first_seen_ms: Some(observed_at_ms),
        last_seen_ms: Some(observed_at_ms),
        absent_since_ms: (presence == DevicePresence::Absent).then_some(observed_at_ms),
    }
}

fn lifecycles(
    device_id: &str,
    presence: DevicePresence,
    generation: u64,
) -> BTreeMap<DeviceId, DeviceLifecycle> {
    BTreeMap::from([(
        DeviceId::new(device_id),
        lifecycle(presence, generation, 10),
    )])
}

fn fan_reading(
    device_id: DeviceId,
    id: String,
    label: String,
    rpm: u64,
    observed_at_ms: u64,
) -> SensorReading {
    SensorReading::from_measurement_observation(
        device_id,
        id,
        label,
        SensorMeasurementObservation::available(
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(rpm),
            observed_at_ms,
        )
        .expect("valid fan fixture"),
    )
}

fn healthy_disk(device_id: &str, generation: u64, activity: f32) -> DiskMetrics {
    taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id(device_id.to_owned())
        .device_generation(DeviceGeneration::new(generation))
        .device_state(DeviceState::healthy(10))
        .scalar_observations(taskmanager_core::DiskScalarObservations {
            active_time_pct: ScalarObservation::available(activity, 10),
            ..Default::default()
        })
        .build()
}

fn healthy_disk_with_rate(
    device_id: &str,
    generation: u64,
    read_bytes_per_sec: u64,
    write_bytes_per_sec: u64,
) -> DiskMetrics {
    taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id(device_id.to_owned())
        .device_generation(DeviceGeneration::new(generation))
        .device_state(DeviceState::healthy(10))
        .scalar_observations(taskmanager_core::DiskScalarObservations {
            read_bytes_per_sec: ScalarObservation::available(read_bytes_per_sec, 10),
            write_bytes_per_sec: ScalarObservation::available(write_bytes_per_sec, 10),
            ..Default::default()
        })
        .build()
}

#[path = "system_history/concurrency.rs"]
mod concurrency;
#[path = "system_history/cpu.rs"]
mod cpu;
#[path = "system_history/devices.rs"]
mod devices;
#[path = "system_history/dynamic_bounds.rs"]
mod dynamic_bounds;
#[path = "system_history/gaps.rs"]
mod gaps;

#[path = "system_history/persistence_sink.rs"]
mod persistence_sink;

#[path = "system_history/split_rates.rs"]
mod split_rates;
