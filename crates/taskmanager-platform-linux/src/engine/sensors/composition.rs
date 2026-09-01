//! Composition boundary for independently fallible Linux sensor providers.
//!
//! The composite sysfs inventory is the sole lifecycle discovery authority.
//! Child hwmon and thermal discovery receipts remain visible as source
//! diagnostics, while a partial child scan can never confirm device absence.

use super::*;
use taskmanager_core::core::sensors::ThermalThrottleSnapshot;
use taskmanager_platform_contract::DeviceDiscovery;

const CPU_THROTTLE_PROVIDER: ProviderId = ProviderId::borrowed("linux.sensor.cpu-thermal-throttle");

pub(super) fn collect_sensor_center_source_from_roots(
    hwmon_root: &Path,
    thermal_root: &Path,
    cpu_root: &Path,
    iio_root: &Path,
    now_ms: u64,
) -> DeviceSourceSnapshot<SensorCenterSnapshot> {
    let hwmon = collect_sensor_center_source_from(hwmon_root, now_ms);
    let thermal = thermal::collect(
        thermal_root,
        &thermal::mirrored_zone_devices(hwmon_root),
        now_ms,
    );
    let throttle = trend::collect_thermal_throttle_from(cpu_root, now_ms);
    let iio = iio::collect_iio_source_from(iio_root, now_ms);
    combine_sensor_sources(hwmon, thermal, throttle, iio, now_ms)
}

fn combine_sensor_sources(
    hwmon: DeviceSourceSnapshot<SensorCenterSnapshot>,
    thermal: thermal::ThermalSourceSnapshot,
    throttle: ThermalThrottleSnapshot,
    iio: DeviceSourceSnapshot<SensorCenterSnapshot>,
    now_ms: u64,
) -> DeviceSourceSnapshot<SensorCenterSnapshot> {
    let throttle_source = throttle_source_status(&throttle);
    let all_readings = hwmon
        .value
        .readings
        .iter()
        .chain(&thermal.readings)
        .chain(&iio.value.readings);
    let has_current_reading = all_readings
        .clone()
        .any(|reading| reading.current_measurement().is_some());
    let has_permission_failure = all_readings.clone().any(|reading| {
        reading.measurement_observation().failure() == Some(FailureKind::PermissionDenied)
    });
    let has_any_reading = all_readings.clone().next().is_some();
    let mut discovered_devices = hwmon.discovered_devices().to_vec();
    discovered_devices.extend(thermal.discovered_devices.iter().cloned());
    discovered_devices.extend(iio.discovered_devices().iter().cloned());
    discovered_devices.sort();
    discovered_devices.dedup();

    let child_discoveries = [
        hwmon.discovery().clone(),
        thermal.discovery.clone(),
        iio.discovery().clone(),
    ];
    let discovery_outcome =
        aggregate_discovery_outcome(&child_discoveries, discovered_devices.len());
    let status = sensor_center_status(
        discovery_outcome,
        has_current_reading,
        has_permission_failure,
        has_any_reading,
        &thermal,
        &throttle,
    );
    let thermal::ThermalSourceSnapshot {
        readings: thermal_readings,
        zones,
        cooling_devices,
        enrichments: thermal_enrichments,
        ..
    } = thermal;
    let mut readings = hwmon.value.readings;
    readings.extend(thermal_readings);
    readings.extend(iio.value.readings);
    readings.sort_by(|left, right| left.id().cmp(right.id()));
    let mut enrichments = child_discoveries.into_iter().collect::<Vec<_>>();
    enrichments.extend(hwmon.enrichments);
    enrichments.extend(thermal_enrichments);
    enrichments.extend(iio.enrichments);
    enrichments.push(throttle_source);

    let discovery = match discovery_outcome {
        SourceOutcome::Available => DeviceDiscovery::Available(discovered_devices),
        SourceOutcome::Empty => DeviceDiscovery::Empty,
        SourceOutcome::Partial(failure) => DeviceDiscovery::Partial {
            discovered_devices,
            failure,
        },
        SourceOutcome::Unavailable(failure) => DeviceDiscovery::Unavailable(failure),
    };
    DeviceSourceSnapshot::from_discovery(
        SensorCenterSnapshot {
            state: DeviceState::default().transition(status, now_ms),
            timestamp_ms: now_ms,
            readings,
            thermal_control: ThermalControlSnapshot {
                zones,
                cooling_devices,
                throttle,
            },
            device_lifecycles: Default::default(),
        },
        SYSFS_INVENTORY_PROVIDER,
        discovery,
        enrichments,
    )
}

fn throttle_source_status(throttle: &ThermalThrottleSnapshot) -> SourceStatus {
    let fields = [
        throttle.core_events_observation().availability(),
        throttle.package_events_observation().availability(),
    ];
    let current = fields
        .iter()
        .filter(|availability| availability.is_current())
        .count();
    let failure = fields
        .into_iter()
        .filter_map(taskmanager_core::ScalarAvailability::failure)
        .max_by_key(|failure| failure_priority(*failure));
    SourceStatus {
        provider: CPU_THROTTLE_PROVIDER,
        outcome: match (current, failure) {
            (2, None) => SourceOutcome::Available,
            (0, Some(failure)) => SourceOutcome::Unavailable(failure),
            (_, Some(failure)) => SourceOutcome::Partial(failure),
            _ => SourceOutcome::Unavailable(FailureKind::ProviderFault),
        },
        item_count: current,
    }
}

fn aggregate_discovery_outcome(sources: &[SourceStatus], discovered_count: usize) -> SourceOutcome {
    let failure = sources
        .iter()
        .filter_map(|source| match source.outcome {
            SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => Some(failure),
            SourceOutcome::Available | SourceOutcome::Empty => None,
        })
        .max_by_key(|failure| failure_priority(*failure));
    match failure {
        Some(failure) if discovered_count == 0 => SourceOutcome::Unavailable(failure),
        Some(failure) => SourceOutcome::Partial(failure),
        None if discovered_count == 0 => SourceOutcome::Empty,
        None => SourceOutcome::Available,
    }
}

pub(super) fn sensor_center_status(
    discovery: SourceOutcome,
    has_current_reading: bool,
    has_permission_failure: bool,
    has_any_reading: bool,
    thermal: &thermal::ThermalSourceSnapshot,
    throttle: &ThermalThrottleSnapshot,
) -> DeviceStatus {
    if matches!(discovery, SourceOutcome::Empty) {
        return DeviceStatus::Healthy;
    }
    if let SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) = discovery {
        return DeviceStatus::from_failure(failure);
    }
    let has_current_control = thermal.zones.iter().any(|zone| {
        zone.label.availability().is_current()
            || zone.mode.availability().is_current()
            || zone.policy.availability().is_current()
            || zone.trip_points.availability.is_current()
    }) || thermal.cooling_devices.iter().any(|device| {
        device.kind.availability().is_current()
            || device.current_state.availability().is_current()
            || device.maximum_state.availability().is_current()
            || device.activity.availability().is_current()
    }) || throttle
        .core_events_observation()
        .availability()
        .is_current()
        || throttle
            .package_events_observation()
            .availability()
            .is_current();
    if has_current_control || has_current_reading {
        DeviceStatus::Healthy
    } else if has_permission_failure {
        DeviceStatus::PermissionDenied
    } else if !has_any_reading {
        DeviceStatus::Unsupported
    } else {
        DeviceStatus::Stale
    }
}
