use taskmanager_core::core::{
    DeviceId, DeviceState, FailureKind, FilesystemHealth, FilesystemHealthStatus, SensorDescriptor,
    SensorMagnitude, SensorMeasurementObservation, SensorReading, SensorScale,
};

use super::{SystemHealthText, filesystem_capacity, sensor_value_vm};

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
        read_only: None,
        error_count: None,
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
