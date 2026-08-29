use super::*;

#[test]
fn classifies_indexed_axis_and_named_channels_without_vendor_tables() {
    for (name, token, identity, quantity, unit) in [
        (
            "in_voltage0_raw",
            "voltage",
            "0",
            SensorQuantity::Voltage,
            SensorUnit::Volt,
        ),
        (
            "in_temp1_raw",
            "temp",
            "1",
            SensorQuantity::Temperature,
            SensorUnit::Celsius,
        ),
        (
            "in_current2_raw",
            "current",
            "2",
            SensorQuantity::Current,
            SensorUnit::Ampere,
        ),
        (
            "in_power3_raw",
            "power",
            "3",
            SensorQuantity::Power,
            SensorUnit::Watt,
        ),
        (
            "in_energy1_raw",
            "energy",
            "1",
            SensorQuantity::Energy,
            SensorUnit::Joule,
        ),
        (
            "in_humidityrelative1_raw",
            "humidityrelative",
            "1",
            SensorQuantity::RelativeHumidity,
            SensorUnit::Percent,
        ),
        (
            "in_accel_x_raw",
            "accel",
            "_x",
            SensorQuantity::Opaque("iio_accel".into()),
            SensorUnit::Opaque("raw_iio_accel".into()),
        ),
        (
            "in_intensity_ir_raw",
            "intensity",
            "_ir",
            SensorQuantity::Opaque("iio_intensity".into()),
            SensorUnit::Opaque("raw_iio_intensity".into()),
        ),
        (
            "in_illuminance0_raw",
            "illuminance",
            "0",
            SensorQuantity::Opaque("iio_illuminance".into()),
            SensorUnit::Opaque("raw_iio_illuminance".into()),
        ),
        (
            "in_voltage0_x_raw",
            "voltage",
            "0_x",
            SensorQuantity::Voltage,
            SensorUnit::Volt,
        ),
    ] {
        let channel = parse_iio_channel(name).expect("standard IIO channel");
        assert_eq!(channel.type_token, token, "{name}");
        assert_eq!(channel.channel, identity, "{name}");
        assert_eq!(channel.descriptor.quantity(), &quantity, "{name}");
        assert_eq!(channel.descriptor.unit(), &unit, "{name}");
    }
}

#[test]
fn scale_file_probes_prefer_per_channel_then_shared_type_scale() {
    let voltage = parse_iio_channel("in_voltage0_raw").expect("indexed channel");
    assert_eq!(voltage.scale_file.as_deref(), Some("in_voltage0_scale"));
    let axis = parse_iio_channel("in_accel_x_raw").expect("axis channel");
    assert_eq!(axis.scale_file.as_deref(), Some("in_accel_x_scale"));
    let indexed_axis = parse_iio_channel("in_voltage0_x_raw").expect("indexed axis");
    assert_eq!(
        indexed_axis.scale_file.as_deref(),
        Some("in_voltage0_x_scale")
    );
}

#[test]
fn non_channel_and_unknown_shaped_files_are_not_readings() {
    for name in [
        "in_timestamp_raw",
        "in_voltage0_scale",
        "in_voltage_scale",
        "in_temp0_offset",
        "name",
        "uevent",
        "power",
        "in_0_raw",
        "in__raw",
    ] {
        assert_eq!(parse_iio_channel(name), None, "{name}");
    }
}

#[test]
fn decimal_scales_become_exact_rationals() {
    for (text, expected) in [
        ("0.1", Some((1, 10))),
        ("1", Some((1, 1))),
        ("2.5", Some((25, 10))),
        ("0.001", Some((1, 1000))),
        ("0.000122070", Some((122070, 1_000_000_000))),
        ("1.25e-3", Some((125, 100_000))),
        ("250e-2", Some((250, 100))),
    ] {
        assert_eq!(parse_decimal_scale(text), expected, "{text}");
    }
    for text in ["0", "0.0", "-0.1", "-1", "abc", "1.2.3", "1e999", "1e-999"] {
        assert_eq!(parse_decimal_scale(text), None, "{text}");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn fake_iio_tree_collects_scaled_known_and_opaque_raw_channels() {
    use std::os::unix::fs::PermissionsExt;

    use taskmanager_core::core::sensors::SensorQuantity;
    use taskmanager_core::core::source::SourceOutcome;

    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm-iio-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let physical = root.join("devices/pci0000:00/0000:00:01.3");
    std::fs::create_dir_all(&physical).expect("physical device");
    let iio_root = root.join("bus/iio/devices");
    let device = iio_root.join("iio:device2");
    std::fs::create_dir_all(&device).expect("iio device");
    std::os::unix::fs::symlink(&physical, device.join("device")).expect("device link");
    std::fs::write(device.join("name"), "hwmon-fixture-bme680\n").expect("device name");
    // Scaled known channels: per-channel scale and shared type scale.
    std::fs::write(device.join("in_temp1_raw"), "31250\n").expect("temperature raw");
    std::fs::write(device.join("in_temp1_scale"), "0.001\n").expect("temperature scale");
    std::fs::write(device.join("in_voltage0_raw"), "32768\n").expect("voltage raw");
    std::fs::write(device.join("in_voltage_scale"), "0.000122070\n").expect("shared voltage scale");
    // Known channel without a parseable scale degrades to opaque raw.
    std::fs::write(device.join("in_power1_raw"), "12500000\n").expect("power raw");
    std::fs::write(device.join("in_power1_scale"), "garbage\n").expect("bad scale");
    // Opaque axis channel stays raw.
    std::fs::write(device.join("in_accel_x_raw"), "-42\n").expect("accel raw");
    // Non-channel files must be ignored.
    std::fs::write(device.join("in_timestamp_raw"), "1234\n").expect("timestamp");
    std::fs::write(device.join("in_temp1_offset"), "0\n").expect("offset");
    // Typed failure path: unreadable raw.
    std::fs::write(device.join("in_current3_raw"), "7\n").expect("current raw");
    std::fs::set_permissions(
        device.join("in_current3_raw"),
        std::fs::Permissions::from_mode(0o000),
    )
    .expect("deny current raw");

    let source = collect_iio_source_from(&iio_root, 500);
    assert_eq!(source.discovery().outcome, SourceOutcome::Available);
    assert_eq!(source.discovered_devices().len(), 1);
    let snapshot = source.value;
    assert_eq!(snapshot.state.status, DeviceStatus::Healthy);
    assert_eq!(snapshot.readings.len(), 5);
    assert!(snapshot.readings.iter().all(|reading| {
        reading.measurement_observation().availability()
            != taskmanager_core::ScalarAvailability::Unknown
    }));
    let temperature = snapshot
        .readings
        .iter()
        .find(|reading| reading.id().ends_with(":temp1"))
        .expect("temperature channel");
    assert_eq!(temperature.current_number(), Some(31.25));
    let voltage = snapshot
        .readings
        .iter()
        .find(|reading| reading.id().ends_with(":voltage0"))
        .expect("voltage channel");
    assert_eq!(
        voltage.measurement_observation().source_scale(),
        Some(SensorScale::ratio(122070, 1_000_000_000).expect("exact rational"))
    );
    let power = snapshot
        .readings
        .iter()
        .find(|reading| reading.id().ends_with(":power1"))
        .expect("power channel degrades to opaque");
    assert_eq!(
        power.quantity(),
        &SensorQuantity::Opaque("iio_power".into())
    );
    assert_eq!(
        power.current_measurement(),
        Some(SensorMagnitude::Signed(12_500_000))
    );
    let current = snapshot
        .readings
        .iter()
        .find(|reading| reading.id().ends_with(":current3"))
        .expect("current channel");
    assert_eq!(
        current.measurement_observation().availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert!(matches!(
        source.enrichments[0].outcome,
        SourceOutcome::Partial(FailureKind::PermissionDenied)
    ));

    std::fs::set_permissions(
        device.join("in_current3_raw"),
        std::fs::Permissions::from_mode(0o644),
    )
    .expect("restore fixture permissions");
    std::fs::remove_dir_all(root).ok();
}
