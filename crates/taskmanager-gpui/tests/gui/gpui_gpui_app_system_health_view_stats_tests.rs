use taskmanager_core::core::{
    DeviceGeneration, DeviceId, DeviceState, DeviceStatus, FailureKind, FilesystemHealth,
    FilesystemHealthStatus, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
    SensorReading, SensorScale,
};

use super::{SensorGroup, SystemHealthText, filesystem_capacity, sensor_rows, sensor_value_vm};

fn copy(text: SystemHealthText) -> String {
    match text {
        SystemHealthText::Unavailable => "n/a".to_string(),
        other => format!("{other:?}"),
    }
}

fn reading(descriptor: SensorDescriptor, value: Option<SensorMagnitude>) -> SensorReading {
    let now = 1_700_000_000_000;
    let observation = value.map_or_else(
        || SensorMeasurementObservation::unavailable(descriptor.clone(), FailureKind::Unsupported),
        |value| {
            SensorMeasurementObservation::available(descriptor.clone(), value, now)
                .expect("valid sensor fixture")
        },
    );
    SensorReading::from_measurement_observation(
        DeviceId::new("stats:test".to_string()),
        "t".into(),
        "label".into(),
        observation,
    )
}

#[test]
fn sensor_vm_formats_kind_matched_values_with_page_conventions() {
    let temperature = sensor_value_vm(
        &reading(
            SensorDescriptor::temperature(SensorScale::IDENTITY),
            Some(SensorMagnitude::Decimal(67.5)),
        ),
        &copy,
    );
    assert_eq!(temperature.text, "67.5 °C");
    assert!(temperature.present);

    let fan = sensor_value_vm(
        &reading(
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            Some(SensorMagnitude::Unsigned(1_380)),
        ),
        &copy,
    );
    assert_eq!(fan.text, "1380 RPM");
    assert!(fan.present);

    let power = sensor_value_vm(
        &reading(
            SensorDescriptor::power(SensorScale::IDENTITY),
            Some(SensorMagnitude::Decimal(42.75)),
        ),
        &copy,
    );
    assert_eq!(power.text, "42.75 W");
    assert!(power.present);
}

#[test]
fn sensor_vm_renders_explicit_unavailability_as_absent() {
    let missing = sensor_value_vm(
        &reading(SensorDescriptor::temperature(SensorScale::IDENTITY), None),
        &copy,
    );
    assert_eq!(missing.text, "n/a");
    assert!(!missing.present);
}

/// One thermal-zone reading as the Linux provider emits it: the label is the
/// zone's `type` (the reading's source name), the device id carries the zone
/// attachment, and the descriptor's source scale is milli-degrees.
fn thermal_zone_reading(
    label: &str,
    device_id: &str,
    channel: &str,
    milli_c: Option<i64>,
    generation: u64,
) -> SensorReading {
    let now = 1_700_000_000_000;
    let descriptor = SensorDescriptor::temperature(SensorScale::MILLI);
    let observation = milli_c.map_or_else(
        || SensorMeasurementObservation::unavailable(descriptor.clone(), FailureKind::Unsupported),
        |value| {
            SensorMeasurementObservation::available(
                descriptor.clone(),
                SensorMagnitude::Signed(value),
                now,
            )
            .expect("valid thermal-zone fixture")
        },
    );
    SensorReading::from_measurement_observation(
        DeviceId::new(device_id.to_string()),
        channel.into(),
        label.into(),
        observation,
    )
    .with_device_generation(DeviceGeneration::new(generation))
}

/// The System page's Health surface (the sensor center) is the thermal surface
/// this shape delivers: the fold traverses the whole shared sensor reading list
/// and emits one row per thermal-zone reading, named by the zone's own type
/// label, carrying the reading's real value and typed status. A sibling fan
/// channel stays in its own group, and a zone whose read failed keeps its row
/// with the typed absence instead of a fabricated "0.0 °C".
#[test]
fn sensor_rows_traverse_every_thermal_zone_reading_and_name_its_source() {
    let fan = SensorReading::from_measurement_observation(
        DeviceId::new("hwmon:cpu".to_string()),
        "fan1".into(),
        "cpu_fan".into(),
        SensorMeasurementObservation::available(
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(2_400),
            1_700_000_000_000,
        )
        .expect("valid fan fixture"),
    )
    .with_device_generation(DeviceGeneration::new(2));
    let readings = vec![
        thermal_zone_reading("acpitz", "thermal:acpitz:zone:0", "zone0", Some(54_500), 2),
        thermal_zone_reading(
            "x86_pkg_temp",
            "thermal:x86_pkg_temp:zone:0",
            "zone1",
            Some(71_000),
            2,
        ),
        fan,
        thermal_zone_reading(
            "thermal_zone_unreadable",
            "thermal:acpitz:zone:1",
            "zone2",
            None,
            2,
        ),
    ];

    let rows = sensor_rows(&readings, SensorGroup::Temperature, &copy);
    assert_eq!(
        rows.len(),
        3,
        "one row per thermal-zone reading; the fan channel must not leak in"
    );
    let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(
        labels,
        ["acpitz", "x86_pkg_temp", "thermal_zone_unreadable"],
        "each row names the reading's own source label, in projection order"
    );
    assert_eq!(rows[0].value, "54.5 °C");
    assert!(rows[0].present);
    assert_eq!(rows[0].status, DeviceStatus::Healthy);
    assert_eq!(rows[1].value, "71.0 °C");
    assert!(rows[1].present);
    assert_eq!(rows[2].value, "n/a");
    assert!(
        !rows[2].present,
        "an unread thermal zone must not fabricate a value"
    );
    assert_eq!(
        rows[2].status,
        DeviceStatus::Unsupported,
        "the row keeps the reading's typed status for the tone/badge"
    );

    let fans = sensor_rows(&readings, SensorGroup::FanSpeed, &copy);
    assert_eq!(fans.len(), 1);
    assert_eq!(fans[0].label, "cpu_fan");
    assert_eq!(fans[0].value, "2400 RPM");
}

#[test]
fn filesystem_capacity_folds_matching_disk_and_reports_missing_states() {
    let now = 1_700_000_000_000;
    let disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .device_id("disk:stats".into())
        .device_state(DeviceState::healthy(now))
        .name("sda".into())
        .mount_point("/".into())
        .current_capacity_bytes(1_024)
        .current_available_bytes(256)
        .build();
    let filesystem = |mount: &str| FilesystemHealth {
        mount_point: mount.into(),
        source: None,
        fs_type: "ext4".into(),
        backing_kind: taskmanager_core::core::storage_health::FilesystemBackingKind::PhysicalBlock,
        read_only: None,
        error_count: None,
        inode_used: None,
        inode_total: None,
        inode_usage_percent: None,
        status: FilesystemHealthStatus::Healthy,
        state: DeviceState::healthy(now),
        integrity_state: DeviceState::default(),
    };

    assert_eq!(
        filesystem_capacity(&filesystem("/"), Some(&disk)),
        Some((75.0, 256))
    );
    assert_eq!(filesystem_capacity(&filesystem("/"), None), None);
    assert_eq!(
        filesystem_capacity(&filesystem("/media/other"), Some(&disk)),
        None
    );

    let mut zeroed = disk.clone();
    let mut observations = *zeroed.scalar_observations();
    observations.capacity_bytes = taskmanager_core::core::ScalarObservation::available(0, now);
    zeroed.apply_scalar_observations(observations);
    assert_eq!(filesystem_capacity(&filesystem("/"), Some(&zeroed)), None);

    let mut unmounted = disk.clone();
    unmounted.mount_point.clear();
    assert_eq!(
        filesystem_capacity(&filesystem("/"), Some(&unmounted)),
        None
    );
}
