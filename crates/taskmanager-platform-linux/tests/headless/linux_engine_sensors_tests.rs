use super::*;
use taskmanager_core::core::sensors::{SensorQuantity, SensorScale, refresh_sensor_center_state};

fn read_fixture(name: &str, result: std::io::Result<String>, now_ms: u64) -> SensorReading {
    let channel = hwmon::parse_channel(name).expect("standard hwmon fixture channel");
    read_sensor(
        DeviceId::new("hwmon:fixture"),
        format!("hwmon:fixture:{name}"),
        name.into(),
        channel.descriptor,
        result,
        now_ms,
    )
}

#[test]
fn strongest_enumeration_failure_is_order_independent() {
    for (left, right, expected) in [
        (
            FailureKind::Unsupported,
            FailureKind::TemporarilyUnavailable,
            FailureKind::TemporarilyUnavailable,
        ),
        (
            FailureKind::TemporarilyUnavailable,
            FailureKind::PermissionDenied,
            FailureKind::PermissionDenied,
        ),
    ] {
        assert_eq!(stronger_failure(Some(left), right), expected);
        assert_eq!(stronger_failure(Some(right), left), expected);
    }
    assert_eq!(
        stronger_failure(None, FailureKind::Unsupported),
        FailureKind::Unsupported
    );
}

#[test]
fn raw_units_keep_exact_magnitude_scale_and_legacy_projection() {
    let temperature = read_fixture("temp1_input", Ok("42500".into()), 10);
    let fan = read_fixture("fan1_input", Ok("0".into()), 10);
    let power = read_fixture("power1_input", Ok("12500000".into()), 10);

    assert_eq!(
        temperature.current_measurement(),
        Some(SensorMagnitude::Signed(42_500))
    );
    assert_eq!(
        temperature.measurement_observation().source_scale(),
        Some(SensorScale::MILLI)
    );
    assert_eq!(temperature.current_number(), Some(42.5));
    assert_eq!(fan.current_number(), Some(0.0));
    assert_eq!(power.current_number(), Some(12.5));
}

#[test]
fn malformed_overflow_and_physical_type_conflicts_are_unavailable() {
    for (name, raw) in [
        ("temp1_input", "-273151"),
        ("temp1_input", "NaN"),
        ("fan1_input", "-1"),
        ("power1_input", "-1"),
        ("humidity1_input", "100001"),
        ("pwm1", "256"),
        ("intrusion0_alarm", "2"),
        ("energy1_input", "18446744073709551616"),
    ] {
        let reading = read_fixture(name, Ok(raw.into()), 10);
        assert_eq!(
            reading.measurement_observation().availability(),
            taskmanager_core::ScalarAvailability::Unavailable(FailureKind::ProviderFault),
            "{name}={raw}"
        );
    }
}

#[test]
fn permission_failure_is_typed_not_zero() {
    let reading = read_fixture(
        "fan1_input",
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        100,
    );
    assert_eq!(reading.current_measurement(), None);
    assert_eq!(reading.state().status, DeviceStatus::PermissionDenied);
    assert_eq!(
        reading.measurement_observation().availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(reading.measurement_observation().last_success_ms(), None);
}

#[test]
fn valid_zero_and_malformed_text_have_distinct_typed_truth() {
    let zero = read_fixture("fan1_input", Ok("0".into()), 100);
    let malformed = read_fixture("fan2_input", Ok("not-a-number".into()), 100);

    assert_eq!(zero.current_number(), Some(0.0));
    assert_eq!(
        zero.measurement_observation().availability(),
        taskmanager_core::ScalarAvailability::Available
    );
    assert_eq!(zero.measurement_observation().last_success_ms(), Some(100));
    assert_eq!(malformed.current_measurement(), None);
    assert_eq!(
        malformed.measurement_observation().availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
}

#[test]
fn disappearing_input_is_temporary_not_zero_or_device_absence() {
    let missing = read_fixture(
        "temp1_input",
        Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
        100,
    );

    assert_eq!(missing.current_measurement(), None);
    assert_eq!(
        missing.measurement_observation().availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
}

#[test]
fn center_failure_preserves_last_success_and_recovery_advances_it() {
    let mut denied = SensorCenterSnapshot {
        state: DeviceState {
            status: DeviceStatus::PermissionDenied,
            last_success_ms: None,
        },
        ..Default::default()
    };
    refresh_sensor_center_state(DeviceState::healthy(100), &mut denied, 200);
    assert_eq!(denied.state.last_success_ms, Some(100));
    let mut recovered = SensorCenterSnapshot {
        state: DeviceState {
            status: DeviceStatus::Healthy,
            last_success_ms: None,
        },
        ..Default::default()
    };
    refresh_sensor_center_state(denied.state, &mut recovered, 300);
    assert_eq!(recovered.state, DeviceState::healthy(300));
}

#[cfg(target_os = "linux")]
#[test]
fn same_name_attachments_and_opaque_channels_remain_distinct_and_visible() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-sensor-identities-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    for index in [1, 2] {
        let chip = root.join(format!("hwmon{index}"));
        std::fs::create_dir_all(&chip).expect("chip directory");
        std::fs::write(chip.join("name"), "same-chip\n").expect("chip name");
        std::fs::write(chip.join("flux_density1_input"), format!("{index}\n"))
            .expect("opaque channel");
    }

    let source = collect_sensor_center_source_from(&root, 10);

    assert_eq!(
        source.discovery().outcome,
        SourceOutcome::Partial(FailureKind::Unsupported)
    );
    assert_eq!(source.discovered_devices().len(), 2);
    assert_ne!(
        source.discovered_devices()[0],
        source.discovered_devices()[1],
        "attachment fallback must not merge same-name hardware"
    );
    assert_eq!(source.value.readings.len(), 2);
    assert!(source.value.readings.iter().all(|reading| matches!(
        reading.quantity(),
        SensorQuantity::Opaque(token) if token == "flux_density"
    )));
    assert!(source.value.readings.iter().all(|reading| {
        reading.measurement_observation().source_scale().is_none()
            && reading.current_measurement().is_some()
    }));
    std::fs::remove_dir_all(root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn denied_channel_enumeration_keeps_discovery_and_fails_enrichment() {
    use std::os::unix::fs::PermissionsExt;

    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-sensor-unreadable-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("hwmon root");
    let chip = root.join("hwmon7");
    std::fs::create_dir(&chip).expect("chip directory");
    std::fs::write(chip.join("name"), "permission-fixture\n").expect("chip name");
    std::fs::set_permissions(&chip, std::fs::Permissions::from_mode(0o000))
        .expect("deny channel enumeration");

    let source = collect_sensor_center_source_from(&root, 10);
    std::fs::set_permissions(&chip, std::fs::Permissions::from_mode(0o755))
        .expect("restore fixture permissions");

    assert_eq!(source.discovered_devices().len(), 1);
    assert!(matches!(
        source.discovery().outcome,
        SourceOutcome::Partial(FailureKind::PermissionDenied)
    ));
    assert!(matches!(
        source.enrichments[0].outcome,
        SourceOutcome::Unavailable(FailureKind::PermissionDenied)
    ));
    assert!(source.value.readings.is_empty());
    std::fs::remove_dir_all(root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn empty_hwmon_inventory_is_authoritative_empty_and_healthy() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-empty-sensors-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("empty hwmon root");
    let source = collect_sensor_center_source_from(&root, 10);
    assert_eq!(source.discovery().outcome, SourceOutcome::Empty);
    assert_eq!(source.value.state, DeviceState::healthy(10));
    taskmanager_platform_conformance::assert_device_discovery_consistent(&source)
        .expect("empty Linux sensor discovery must be coherent");
    std::fs::remove_dir_all(root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn fake_hwmon_tree_collects_dynamic_channels_with_stable_physical_ids() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-sensors-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let physical = root.join("devices/pci0000:00/0000:00:01.0");
    std::fs::create_dir_all(&physical).unwrap();
    let hwmon_root = root.join("class/hwmon");
    let chip = hwmon_root.join("hwmon2");
    std::fs::create_dir_all(&chip).unwrap();
    std::os::unix::fs::symlink(&physical, chip.join("device")).unwrap();
    std::fs::write(chip.join("name"), "coretemp\n").unwrap();
    std::fs::write(chip.join("temp64_input"), "44000\n").unwrap();
    std::fs::write(chip.join("temp64_label"), "Package id 0\n").unwrap();
    std::fs::write(chip.join("fan257_input"), "1350\n").unwrap();
    std::fs::write(chip.join("power4096_input"), "23000000\n").unwrap();
    std::fs::write(chip.join("temp65_input"), "-273151\n").unwrap();
    std::fs::write(chip.join("fan258_input"), "-1\n").unwrap();
    std::fs::write(chip.join("power4097_input"), "-1\n").unwrap();
    std::fs::write(chip.join("in0_input"), "12000\n").unwrap();
    std::fs::write(chip.join("curr1_input"), "-1500\n").unwrap();
    std::fs::write(chip.join("energy1_input"), "2500000\n").unwrap();
    std::fs::write(chip.join("humidity1_input"), "45500\n").unwrap();
    std::fs::write(chip.join("pwm1"), "128\n").unwrap();
    std::fs::write(chip.join("intrusion0_alarm"), "1\n").unwrap();

    let first_source = collect_sensor_center_source_from(&hwmon_root, 500);
    assert_eq!(first_source.discovery().outcome, SourceOutcome::Available);
    assert_eq!(first_source.discovered_devices().len(), 1);
    assert!(matches!(
        first_source.enrichments[0].outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    ));
    let first = first_source.value;
    assert_eq!(first.state, DeviceState::healthy(500));
    assert_eq!(first.readings.len(), 12);
    assert!(first.readings.iter().any(|reading| {
        reading.label() == "Package id 0" && reading.current_number() == Some(44.0)
    }));
    assert!(
        first
            .readings
            .iter()
            .any(|reading| reading.current_number() == Some(23.0))
    );
    assert_eq!(
        first
            .readings
            .iter()
            .filter(|reading| reading.current_measurement().is_none())
            .count(),
        3
    );
    assert!(first.readings.iter().all(|reading| {
        reading.measurement_observation().availability()
            != taskmanager_core::ScalarAvailability::Unknown
    }));
    let voltage = first
        .readings
        .iter()
        .find(|reading| reading.quantity() == &SensorQuantity::Voltage)
        .expect("voltage channel");
    assert_eq!(
        voltage.measurement_observation().current_number(),
        Some(12.0)
    );
    assert!(
        first
            .readings
            .iter()
            .any(|reading| reading.measurement_observation().current_value()
                == Some(&SensorMagnitude::Boolean(true)))
    );
    assert!(first.readings.iter().any(|reading| {
        reading.measurement_observation().current_value()
            == Some(&SensorMagnitude::DutyCycle {
                value: 128,
                maximum: 255,
            })
    }));

    let first_ids = first
        .readings
        .iter()
        .map(|reading| reading.id().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        first
            .readings
            .iter()
            .map(|reading| reading.device_id().as_str())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        1
    );
    let first_device_ids = first
        .readings
        .iter()
        .map(|reading| reading.device_id().clone())
        .collect::<Vec<_>>();
    let broken = hwmon_root.join("hwmon999");
    std::os::unix::fs::symlink(root.join("missing-device"), &broken).unwrap();
    let partial_source = collect_sensor_center_source_from(&hwmon_root, 550);
    assert!(matches!(
        partial_source.discovery().outcome,
        SourceOutcome::Partial(FailureKind::TemporarilyUnavailable)
    ));
    let partial = partial_source.value;
    assert_eq!(partial.state.status, DeviceStatus::Stale);
    assert_eq!(partial.readings.len(), first.readings.len());
    std::fs::remove_file(broken).unwrap();

    std::fs::rename(&chip, hwmon_root.join("hwmon987")).unwrap();
    let reordered = collect_sensor_center_from(&hwmon_root, 600);
    let reordered_ids = reordered
        .readings
        .iter()
        .map(|reading| reading.id().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(reordered_ids, first_ids);
    assert_eq!(
        reordered
            .readings
            .iter()
            .map(|reading| reading.device_id().clone())
            .collect::<Vec<_>>(),
        first_device_ids
    );
    assert!(
        reordered_ids
            .iter()
            .all(|id| !id.contains("hwmon2") && !id.contains("hwmon987"))
    );
    std::fs::remove_dir_all(root).ok();
}
