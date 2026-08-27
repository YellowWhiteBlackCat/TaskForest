use super::*;

#[cfg(target_os = "linux")]
fn empty_thermal_source() -> thermal::ThermalSourceSnapshot {
    thermal::ThermalSourceSnapshot {
        readings: Vec::new(),
        zones: Vec::new(),
        cooling_devices: Vec::new(),
        discovered_devices: Vec::new(),
        discovery: SourceStatus {
            provider: thermal::DISCOVERY_PROVIDER,
            outcome: SourceOutcome::Empty,
            item_count: 0,
        },
        enrichments: Vec::new(),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn sensor_center_status_preserves_discovery_and_current_data_truth() {
    let thermal = empty_thermal_source();
    let throttle = trend::ThermalThrottleSnapshot::default();
    for (discovery, current, denied, any, expected) in [
        (
            SourceOutcome::Empty,
            false,
            false,
            false,
            DeviceStatus::Healthy,
        ),
        (
            SourceOutcome::Available,
            true,
            false,
            true,
            DeviceStatus::Healthy,
        ),
        (
            SourceOutcome::Available,
            false,
            true,
            true,
            DeviceStatus::PermissionDenied,
        ),
        (
            SourceOutcome::Available,
            false,
            false,
            true,
            DeviceStatus::Stale,
        ),
        (
            SourceOutcome::Partial(FailureKind::MissingDependency),
            true,
            false,
            true,
            DeviceStatus::MissingTool,
        ),
    ] {
        assert_eq!(
            composition::sensor_center_status(discovery, current, denied, any, &thermal, &throttle,),
            expected
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn combined_sysfs_inventory_adds_unmirrored_thermal_zones_once() {
    use std::os::unix::fs::symlink;

    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-sensor-sysfs-composite-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let hwmon_root = root.join("class/hwmon");
    let thermal_root = root.join("class/thermal");
    let cpu_root = root.join("devices/system/cpu");
    std::fs::create_dir_all(&hwmon_root).expect("hwmon root");
    std::fs::create_dir_all(&thermal_root).expect("thermal root");

    let mirrored = root.join("devices/virtual/thermal/thermal_zone0");
    let mirrored_hwmon = mirrored.join("hwmon1");
    std::fs::create_dir_all(&mirrored_hwmon).expect("mirrored hwmon");
    std::fs::write(mirrored.join("type"), "acpitz\n").expect("mirrored type");
    std::fs::write(mirrored.join("temp"), "44000\n").expect("mirrored temperature");
    std::fs::write(mirrored.join("mode"), "enabled\n").expect("mirrored mode");
    std::fs::write(mirrored.join("policy"), "step_wise\n").expect("mirrored policy");
    std::fs::write(mirrored.join("trip_point_0_type"), "critical\n").expect("trip type");
    std::fs::write(mirrored.join("trip_point_0_temp"), "105000\n").expect("trip temperature");
    std::fs::write(mirrored.join("trip_point_0_hyst"), "0\n").expect("trip hysteresis");
    std::fs::write(mirrored_hwmon.join("name"), "acpitz\n").expect("hwmon name");
    std::fs::write(mirrored_hwmon.join("temp1_input"), "44000\n").expect("hwmon temperature");
    symlink(&mirrored_hwmon, hwmon_root.join("hwmon1")).expect("hwmon link");
    symlink(&mirrored, thermal_root.join("thermal_zone0")).expect("thermal link");

    let thermal_only = root.join("devices/virtual/thermal/thermal_zone8");
    std::fs::create_dir_all(&thermal_only).expect("thermal-only zone");
    std::fs::write(thermal_only.join("type"), "iwlwifi_1\n").expect("thermal-only type");
    std::fs::write(thermal_only.join("temp"), "57000\n").expect("thermal-only temperature");
    std::fs::write(thermal_only.join("mode"), "enabled\n").expect("thermal-only mode");
    std::fs::write(thermal_only.join("policy"), "step_wise\n").expect("thermal-only policy");
    symlink(&thermal_only, thermal_root.join("thermal_zone8")).expect("thermal-only link");

    let cooling = root.join("devices/virtual/thermal/cooling_device2");
    std::fs::create_dir_all(&cooling).expect("cooling device");
    std::fs::write(cooling.join("type"), "Fan\n").expect("cooling type");
    std::fs::write(cooling.join("cur_state"), "1\n").expect("cooling current state");
    std::fs::write(cooling.join("max_state"), "1\n").expect("cooling maximum state");
    symlink(&cooling, thermal_root.join("cooling_device2")).expect("cooling link");

    let throttle = cpu_root.join("cpu0/thermal_throttle");
    std::fs::create_dir_all(&throttle).expect("CPU throttle root");
    std::fs::write(throttle.join("core_throttle_count"), "3\n").expect("core throttle count");
    std::fs::write(throttle.join("package_throttle_count"), "7\n").expect("package throttle count");

    let source = composition::collect_sensor_center_source_from_roots(
        &hwmon_root,
        &thermal_root,
        &cpu_root,
        Path::new("/nonexistent-iio-root"),
        700,
    );

    assert_eq!(source.discovery().provider, SYSFS_INVENTORY_PROVIDER);
    assert_eq!(
        source.discovery().outcome,
        SourceOutcome::Partial(FailureKind::Unsupported),
        "enumerated hwmon attachment names must expose identity degradation"
    );
    assert_eq!(source.discovered_devices().len(), 3);
    assert_eq!(source.value.readings.len(), 2);
    assert_eq!(
        source
            .value
            .readings
            .iter()
            .filter(|reading| {
                reading.current_measurement() == Some(SensorMagnitude::Signed(44_000))
            })
            .count(),
        1,
        "a thermal zone mirrored by hwmon must not be emitted twice"
    );
    assert!(source.value.readings.iter().any(|reading| {
        reading.label() == "iwlwifi_1"
            && reading.current_measurement() == Some(SensorMagnitude::Signed(57_000))
    }));
    assert!(source.enrichments.iter().any(|status| {
        status.provider == thermal::DISCOVERY_PROVIDER
            && status.outcome == SourceOutcome::Available
            && status.item_count == 3
    }));
    assert_eq!(source.value.thermal_control.zones.len(), 2);
    assert_eq!(source.value.thermal_control.cooling_devices.len(), 1);
    assert_eq!(
        source.value.thermal_control.throttle.current_core_events(),
        Some(3)
    );
    assert_eq!(
        source
            .value
            .thermal_control
            .throttle
            .current_package_events(),
        Some(7)
    );
    assert!(source.value.thermal_control.zones.iter().any(|zone| {
        source
            .value
            .readings
            .iter()
            .any(|reading| reading.device_id() == &zone.device_id)
    }));
    std::fs::remove_dir_all(root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn live_sensor_source_receipt_preserves_typed_truth() {
    let source = collect_sensor_center_source(1);
    let thermal_discovery = source
        .enrichments
        .iter()
        .find(|status| status.provider == thermal::DISCOVERY_PROVIDER)
        .expect("thermal discovery receipt");
    let thermal_readings = source
        .value
        .readings
        .iter()
        .filter(|reading| reading.device_id().as_str().starts_with("thermal:"))
        .count();
    let zones = source.value.thermal_control.zones.len();
    let cooling = source.value.thermal_control.cooling_devices.len();
    let trip_points = source
        .value
        .thermal_control
        .zones
        .iter()
        .map(|zone| zone.trip_points.points.len())
        .sum::<usize>();
    let throttle = &source.value.thermal_control.throttle;

    eprintln!(
        "live Linux sensor receipt: inventory={:?}, devices={}, thermal={:?}, thermal_readings={thermal_readings}, zones={zones}, cooling={cooling}, trips={trip_points}, throttle_core={:?}, throttle_package={:?}",
        source.discovery().outcome,
        source.discovered_devices().len(),
        thermal_discovery.outcome,
        throttle.core_events_observation().availability(),
        throttle.package_events_observation().availability(),
    );
    assert_eq!(source.discovery().provider, SYSFS_INVENTORY_PROVIDER);
    assert!(source.value.readings.iter().all(|reading| {
        reading.measurement_observation().availability()
            != taskmanager_core::ScalarAvailability::Unknown
    }));
    assert_eq!(
        source
            .discovered_devices()
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        source.discovered_devices().len()
    );
    assert!(source.value.thermal_control.zones.iter().all(|zone| {
        zone.label.availability() != taskmanager_core::ScalarAvailability::Unknown
            && zone.mode.availability() != taskmanager_core::ScalarAvailability::Unknown
            && zone.policy.availability() != taskmanager_core::ScalarAvailability::Unknown
    }));
    assert!(
        source
            .value
            .thermal_control
            .cooling_devices
            .iter()
            .all(|device| {
                device.kind.availability() != taskmanager_core::ScalarAvailability::Unknown
                    && device.current_state.availability()
                        != taskmanager_core::ScalarAvailability::Unknown
                    && device.maximum_state.availability()
                        != taskmanager_core::ScalarAvailability::Unknown
                    && device.activity.availability()
                        != taskmanager_core::ScalarAvailability::Unknown
            })
    );
}
