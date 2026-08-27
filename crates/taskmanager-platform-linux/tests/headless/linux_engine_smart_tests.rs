use super::*;

#[test]
fn provider_stderr_recognises_common_access_failures() {
    assert!(stderr_is_permission_denied(
        b"open device: Permission denied"
    ));
    assert!(stderr_is_permission_denied(
        b"ioctl: Operation not permitted"
    ));
    assert!(stderr_is_permission_denied(b"Insufficient privileges"));
    assert!(!stderr_is_permission_denied(b"device not found"));
}

#[test]
fn controller_from_namespace() {
    assert_eq!(nvme_controller_from_name("nvme0n1"), Some("nvme0".into()));
    assert_eq!(nvme_controller_from_name("nvme10n2"), Some("nvme10".into()));
    assert_eq!(
        nvme_controller_from_name("/dev/nvme3n1"),
        Some("nvme3".into())
    );
    assert_eq!(nvme_controller_from_name("nvme0"), Some("nvme0".into()));
    assert_eq!(nvme_controller_from_name("sda"), None);
    assert_eq!(nvme_controller_from_name("nvme"), None);
}

#[test]
fn parse_leading_handles_percent_and_unit_suffixes() {
    assert_eq!(parse_leading_f32("0%"), Some(0.0));
    assert_eq!(parse_leading_f32("2.5 %"), Some(2.5));
    // nvme-cli temperature token carries a °C suffix.
    assert_eq!(parse_leading_f32("39°C (312.15 K)"), Some(39.0));
    assert_eq!(parse_leading_u64("1234"), Some(1234));
    // smart-log sometimes appends a comma / units after the number.
    assert_eq!(parse_leading_u64("1234,"), Some(1234));
}

/// sysfs parse path against a fake controller dir shaped like this host's
/// `/sys/class/nvme/nvme0/hwmon5` (millidegrees). Confirms temp + alarm +
/// critical threshold all flow through.
///
/// Linux-only: mirrors the `/sys/class/nvme` layout (a Linux kernel nvme
/// driver sysfs root), so it exercises the cfg(linux)-gated sysfs readers
/// (`read_milli`/`read_u64`). Off-Linux both the readers and this mirror are
/// compiled out — the pure-parser tests below stay cross-platform.
#[cfg(target_os = "linux")]
#[test]
fn read_sysfs_hwmon_parses_composite_temp_alarm_crit() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_smart_sysfs_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // Mirror the real layout: <root>/nvme0/hwmon0/{temp1_*}.
    let hw = root.join("nvme0").join("hwmon0");
    std::fs::create_dir_all(&hw).unwrap();
    std::fs::write(hw.join("temp1_input"), "34850\n").unwrap(); // 34.85 °C
    std::fs::write(hw.join("temp1_alarm"), "0\n").unwrap();
    std::fs::write(hw.join("temp1_crit"), "86850\n").unwrap(); // 86.85 °C

    // Point the helper at the fake root by reading the per-controller dir.
    let observation = read_sysfs_hwmon_in(&root, "nvme0");
    assert_eq!(observation.failure, None);
    let smart = observation.value.expect("valid hwmon observation");
    assert_eq!(smart.availability, SmartAvailability::Available);
    assert!((smart.temperature_c.unwrap() - 34.85).abs() < 0.01);
    assert_eq!(smart.critical_warning, Some(false));
    assert!((smart.temp_critical_c.unwrap() - 86.85).abs() < 0.01);
    assert_eq!(smart.percent_used, None);
    assert_eq!(smart.power_on_hours, None);

    std::fs::remove_dir_all(&root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn read_sysfs_hwmon_preserves_missing_alarm_as_unknown() {
    let root = smart_sysfs_fixture_root("missing_alarm");
    let hw = root.join("nvme0").join("hwmon0");
    std::fs::create_dir_all(&hw).unwrap();
    std::fs::write(hw.join("temp1_input"), "34850\n").unwrap();

    let observation = read_sysfs_hwmon_in(&root, "nvme0");
    assert_eq!(observation.failure, None);
    let smart = observation.value.expect("temperature remains available");
    assert_eq!(smart.critical_warning, None);

    std::fs::remove_dir_all(&root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn read_sysfs_hwmon_maps_reported_alarm_one_to_warning() {
    let root = smart_sysfs_fixture_root("alarm_one");
    let hw = root.join("nvme0").join("hwmon0");
    std::fs::create_dir_all(&hw).unwrap();
    std::fs::write(hw.join("temp1_input"), "34850\n").unwrap();
    std::fs::write(hw.join("temp1_alarm"), "1\n").unwrap();

    let observation = read_sysfs_hwmon_in(&root, "nvme0");
    assert_eq!(observation.failure, None);
    let smart = observation.value.expect("temperature remains available");
    assert_eq!(smart.critical_warning, Some(true));

    std::fs::remove_dir_all(&root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn read_sysfs_hwmon_rejects_invalid_alarm_value_as_unknown() {
    let root = smart_sysfs_fixture_root("invalid_alarm");
    let hw = root.join("nvme0").join("hwmon0");
    std::fs::create_dir_all(&hw).unwrap();
    std::fs::write(hw.join("temp1_input"), "34850\n").unwrap();
    std::fs::write(hw.join("temp1_alarm"), "2\n").unwrap();

    let observation = read_sysfs_hwmon_in(&root, "nvme0");
    assert_eq!(
        observation.failure,
        Some(SmartProviderFailureKind::MalformedResponse)
    );
    let smart = observation.value.expect("temperature remains available");
    assert_eq!(smart.critical_warning, None);
    assert_eq!(
        smart.failure,
        Some(SmartProviderFailureKind::MalformedResponse)
    );

    std::fs::remove_dir_all(&root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn read_sysfs_hwmon_reports_malformed_temperature_instead_of_empty() {
    let root = smart_sysfs_fixture_root("malformed_temperature");
    let hw = root.join("nvme0").join("hwmon0");
    std::fs::create_dir_all(&hw).unwrap();
    std::fs::write(hw.join("temp1_input"), "not-a-temperature\n").unwrap();

    let observation = read_sysfs_hwmon_in(&root, "nvme0");

    assert!(observation.value.is_none());
    assert_eq!(
        observation.failure,
        Some(SmartProviderFailureKind::MalformedResponse)
    );
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(target_os = "linux")]
#[test]
fn read_sysfs_hwmon_reports_missing_controller_as_unsupported() {
    let root = smart_sysfs_fixture_root("missing_controller");

    let observation = read_sysfs_hwmon_in(&root, "nvme404");

    assert!(observation.value.is_none());
    assert_eq!(
        observation.failure,
        Some(SmartProviderFailureKind::UnsupportedProtocol)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn smart_sysfs_io_errors_keep_typed_failure_reasons() {
    assert_eq!(
        smart_io_failure(&std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        SmartProviderFailureKind::PermissionDenied
    );
    assert_eq!(
        smart_io_failure(&std::io::Error::from(std::io::ErrorKind::TimedOut)),
        SmartProviderFailureKind::TimedOut
    );
}

#[cfg(target_os = "linux")]
fn smart_sysfs_fixture_root(case: &str) -> std::path::PathBuf {
    crate::test_support::repo_temp_dir().join(format!(
        "tm_smart_sysfs_{case}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

/// smart-log parse path against a captured stdout (no `nvme` binary needed).
/// Drives the extracted parser directly — the `Command` shell-out is the
/// only part not exercisable here, and the parsing is the load-bearing logic.
#[test]
fn smart_log_parse_extracts_fields() {
    let log = "\
Smart Log for NVME device:nvme0n1 version-id:1 ...
critical_warning                        : 0
temperature                             : 39°C (312.15 K)
available_spare                         : 100%
available_spare_threshold               : 10%
percentage_used                         : 2%
data_units_read                         : 1,234,567
power_on_hours                          : 5432
";
    let out = parse_smart_log_stdout(log).expect("fields parsed");
    assert_eq!(out.availability, SmartAvailability::Available);
    assert!(
        (out.temperature_c.unwrap() - (39.0_f32)).abs() < 0.5,
        "39 °C parsed"
    );
    assert_eq!(out.critical_warning, Some(false));
    assert!((out.percent_used.unwrap() - 2.0).abs() < 1e-6);
    assert_eq!(out.power_on_hours, Some(5432));
}

/// `critical_warning` is emitted by nvme-cli in hex (`0x4`) when the
/// temperature-alarm bit is set. The parser must surface that as
/// `Some(true)` rather than silently dropping the field.
#[test]
fn smart_log_parse_critical_warning_hex_nonzero() {
    let log = "\
Smart Log for NVME device:nvme0n1 version-id:1 ...
critical_warning                        : 0x4
temperature                             : 41°C (314.15 K)
";
    let out = parse_smart_log_stdout(log).expect("fields parsed");
    assert_eq!(out.critical_warning, Some(true));
    assert!((out.temperature_c.unwrap() - 41.0_f32).abs() < 0.5);
}

/// A body with no recognisable key:value lines yields `None` so callers
/// treat the controller as having no SMART data rather than an all-`None`
/// (and therefore misleadingly `Some`) snapshot.
#[test]
fn smart_log_parse_empty_body_is_none() {
    // `DiskSmart` isn't `PartialEq`, so probe via `Option::is_none`.
    assert!(parse_smart_log_stdout("").is_none());
    // Header-only / unparseable noise must also collapse to `None`.
    assert!(parse_smart_log_stdout("Smart Log for NVME device:nvme0n1 ...").is_none());
}
