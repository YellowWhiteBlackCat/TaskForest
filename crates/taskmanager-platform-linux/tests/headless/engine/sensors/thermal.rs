use super::*;

fn fixture_root(name: &str) -> PathBuf {
    crate::test_support::repo_temp_dir().join(format!(
        "tm-thermal-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ))
}

#[test]
fn thermal_zone_value_and_malformed_sibling_keep_partial_source_truth() {
    let root = fixture_root("partial");
    for (zone, kind, value) in [
        ("thermal_zone7", "x86_pkg_temp", "52000\n"),
        ("thermal_zone9", "iwlwifi_1", "not-a-number\n"),
    ] {
        let directory = root.join(zone);
        fs::create_dir_all(&directory).expect("zone directory");
        fs::write(directory.join("type"), kind).expect("zone type");
        fs::write(directory.join("temp"), value).expect("zone temp");
    }

    let snapshot = collect(&root, &HashMap::new(), 90);

    assert_eq!(snapshot.discovery.outcome, SourceOutcome::Available);
    assert_eq!(snapshot.discovered_devices.len(), 2);
    assert_eq!(
        snapshot.enrichments[1].outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(
        snapshot.readings[0]
            .measurement_observation()
            .availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
    assert!(snapshot.readings.iter().any(|reading| {
        reading.label() == "x86_pkg_temp"
            && reading.current_measurement() == Some(SensorMagnitude::Signed(52_000))
    }));
    fs::remove_dir_all(root).ok();
}

#[test]
fn unique_type_identity_survives_attachment_renumbering() {
    let root = fixture_root("renumber");
    let first = root.join("thermal_zone2");
    fs::create_dir_all(&first).expect("zone directory");
    fs::write(first.join("type"), "iwlwifi_1\n").expect("zone type");
    fs::write(first.join("temp"), "43000\n").expect("zone temp");
    let first_id = collect(&root, &HashMap::new(), 10).discovered_devices[0].clone();

    fs::rename(&first, root.join("thermal_zone91")).expect("renumber zone");
    let second_id = collect(&root, &HashMap::new(), 20).discovered_devices[0].clone();

    assert_eq!(first_id, second_id);
    assert_eq!(first_id.as_str(), "thermal:type:iwlwifi_1");
    fs::remove_dir_all(root).ok();
}

#[test]
fn duplicate_type_without_physical_identity_is_typed_partial_discovery() {
    let root = fixture_root("duplicate-type");
    for zone in ["thermal_zone2", "thermal_zone91"] {
        let directory = root.join(zone);
        fs::create_dir_all(&directory).expect("zone directory");
        fs::write(directory.join("type"), "acpitz\n").expect("zone type");
        fs::write(directory.join("temp"), "43000\n").expect("zone temp");
    }

    let snapshot = collect(&root, &HashMap::new(), 20);

    assert_eq!(snapshot.discovered_devices.len(), 2);
    assert_ne!(
        snapshot.discovered_devices[0], snapshot.discovered_devices[1],
        "attachment fallback must keep ambiguous zones distinct"
    );
    assert_eq!(
        snapshot.discovery.outcome,
        SourceOutcome::Partial(FailureKind::Unsupported)
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn duplicate_cooling_type_without_physical_identity_is_typed_partial_discovery() {
    let root = fixture_root("duplicate-cooling-type");
    for device in ["cooling_device2", "cooling_device91"] {
        let directory = root.join(device);
        fs::create_dir_all(&directory).expect("cooling device directory");
        fs::write(directory.join("type"), "Fan\n").expect("cooling device type");
        fs::write(directory.join("cur_state"), "1\n").expect("current cooling state");
        fs::write(directory.join("max_state"), "3\n").expect("maximum cooling state");
    }

    let snapshot = collect(&root, &HashMap::new(), 20);

    assert_eq!(snapshot.discovered_devices.len(), 2);
    assert_ne!(
        snapshot.discovered_devices[0], snapshot.discovered_devices[1],
        "attachment fallback must keep ambiguous cooling devices distinct"
    );
    assert_eq!(
        snapshot.discovery.outcome,
        SourceOutcome::Partial(FailureKind::Unsupported)
    );
    fs::remove_dir_all(root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn multiple_zones_on_one_physical_device_keep_distinct_channels() {
    use std::os::unix::fs::symlink;

    let root = fixture_root("multi-zone-device");
    let physical = root.join("devices/platform/controller0");
    fs::create_dir_all(&physical).expect("physical device");
    for (zone, kind, value) in [
        ("thermal_zone3", "controller_package", "42000\n"),
        ("thermal_zone4", "controller_hotspot", "47000\n"),
    ] {
        let directory = root.join(zone);
        fs::create_dir_all(&directory).expect("zone directory");
        fs::write(directory.join("type"), kind).expect("zone type");
        fs::write(directory.join("temp"), value).expect("zone temp");
        symlink(&physical, directory.join("device")).expect("physical device link");
    }

    let snapshot = collect(&root, &HashMap::new(), 50);

    assert_eq!(snapshot.discovered_devices.len(), 1);
    assert_eq!(snapshot.readings.len(), 2);
    assert_ne!(snapshot.readings[0].id(), snapshot.readings[1].id());
    assert_eq!(
        snapshot.readings[0].device_id(),
        snapshot.readings[1].device_id()
    );
    assert_eq!(snapshot.zones.len(), 2);
    assert_ne!(snapshot.zones[0].id, snapshot.zones[1].id);
    assert_eq!(snapshot.zones[0].device_id, snapshot.zones[1].device_id);
    fs::remove_dir_all(root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn hwmon_thermal_ancestor_is_detected_as_a_mirror() {
    use std::os::unix::fs::symlink;

    let root = fixture_root("mirror");
    let zone = root.join("devices/virtual/thermal/thermal_zone4");
    let chip = zone.join("hwmon7");
    fs::create_dir_all(&chip).expect("thermal hwmon directory");
    let hwmon = root.join("class/hwmon");
    fs::create_dir_all(&hwmon).expect("hwmon root");
    symlink(&chip, hwmon.join("hwmon7")).expect("hwmon attachment");

    let mirrored = mirrored_zone_devices(&hwmon);
    assert_eq!(mirrored.len(), 1);
    assert!(
        mirrored
            .get(&zone)
            .is_some_and(|device| device.as_str().starts_with("hwmon:"))
    );
    fs::remove_dir_all(root).ok();
}
