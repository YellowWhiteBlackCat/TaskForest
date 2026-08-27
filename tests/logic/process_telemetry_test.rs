use taskmanager::core::{
    DeviceStatus, ProcessTelemetrySnapshot, SensorDescriptor, SensorMagnitude,
    SensorMeasurementObservation, SensorScale, SensorUnit,
};
// The mountinfo/smartctl parsers are Linux-provider helpers (Linux-only
// dev-dependency); the defaults/serialization tests below are neutral.
#[cfg(target_os = "linux")]
use taskmanager::core::{FilesystemHealthStatus, SmartSelfTestKind, SmartSelfTestPhase};
#[cfg(target_os = "linux")]
use taskmanager_platform_linux::{
    parse_mountinfo, parse_smart_self_test_json, smart_self_test_plan,
};

#[test]
fn public_process_telemetry_defaults_do_not_invent_byte_rates() {
    let snapshot = ProcessTelemetrySnapshot::default();
    assert_eq!(snapshot.state.status, DeviceStatus::Unsupported);
    assert_eq!(snapshot.network.rx_bytes_per_sec, None);
    assert_eq!(snapshot.network.tx_bytes_per_sec, None);
    assert!(snapshot.gpu.devices.is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn public_storage_and_self_test_parsers_are_provider_string_free() {
    let filesystems = parse_mountinfo("1 0 8:1 / / ro,relatime - ext4 /dev/sda rw\n", 100);
    assert_eq!(filesystems[0].status, FilesystemHealthStatus::ReadOnly);

    let report =
        parse_smart_self_test_json(include_str!("../fixtures/smartctl_selftest_nvme.json"))
            .unwrap();
    assert_eq!(report.phase, SmartSelfTestPhase::Completed);
    assert_eq!(report.kind, Some(SmartSelfTestKind::Extended));

    let plan = smart_self_test_plan("nvme0n1", SmartSelfTestKind::Short).unwrap();
    assert_eq!(plan.disk_name(), "nvme0n1");
    assert_eq!(plan.kind(), SmartSelfTestKind::Short);
}

#[test]
fn sensor_measurements_serialize_with_explicit_units() {
    let observation = SensorMeasurementObservation::available(
        SensorDescriptor::power(SensorScale::IDENTITY),
        SensorMagnitude::Decimal(12.5),
        10,
    )
    .expect("valid power observation");
    let encoded = serde_json::to_value(observation).expect("serialize power observation");
    assert_eq!(encoded["descriptor"]["unit"], "watt");
    assert_eq!(SensorUnit::Watt.as_token(), "watt");
}
