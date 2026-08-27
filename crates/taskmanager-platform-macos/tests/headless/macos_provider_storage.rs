use super::*;

#[test]
fn smartctl_tokens_are_stable() {
    assert_eq!(smartctl_token(SmartSelfTestKind::Short), "short");
    assert_eq!(smartctl_token(SmartSelfTestKind::Extended), "long");
    assert_eq!(smartctl_token(SmartSelfTestKind::Conveyance), "conveyance");
}

#[test]
fn self_test_phase_parsing_is_stable() {
    assert_eq!(
        parse_phase("Completed without error"),
        SmartSelfTestPhase::Completed
    );
    assert_eq!(parse_phase("Aborted by host"), SmartSelfTestPhase::Aborted);
    assert_eq!(parse_phase("Failed"), SmartSelfTestPhase::Failed);
    assert_eq!(parse_phase("40% remaining"), SmartSelfTestPhase::Running);
    assert_eq!(parse_phase("in progress"), SmartSelfTestPhase::Running);
    assert_eq!(parse_phase("unknown state"), SmartSelfTestPhase::Unknown);
}

#[test]
fn smart_json_maps_nvme_fields() {
    let json: serde_json::Value = serde_json::json!({
        "temperature": { "current": 42 },
        "power_on_time": { "hours": 1234 },
        "nvme_smart_health_information_log": {
            "percentage_used": 12,
            "critical_warning": 3
        }
    });
    let mut row = taskmanager_core::DiskMetrics::default();
    apply_smart_json(&mut row, &json, 10);
    assert_eq!(
        row.smart_availability,
        taskmanager_core::metrics::SmartAvailability::Available
    );
    assert_eq!(row.smart_temperature_c, Some(42.0));
    assert_eq!(row.smart_power_on_hours, Some(1234));
    assert_eq!(row.smart_percent_used, Some(12.0));
    assert_eq!(row.smart_critical_warning, Some(true));
}

#[test]
fn smart_json_absent_sections_stay_absent() {
    let json: serde_json::Value = serde_json::json!({ "model_name": "x" });
    let mut row = taskmanager_core::DiskMetrics::default();
    apply_smart_json(&mut row, &json, 10);
    assert_eq!(
        row.smart_availability,
        taskmanager_core::metrics::SmartAvailability::Available
    );
    assert_eq!(row.smart_temperature_c, None);
    assert_eq!(row.smart_critical_warning, None);
}

#[test]
fn iostat_data_line_validates_token_count() {
    let one_disk = vec!["disk0".to_string()];
    // Two tokens where three are expected -> reject without panicking.
    assert!(parse_iostat_data_line(&["1.0", "2.0"], &one_disk).is_none());
    // A non-numeric token in a numeric slot -> reject.
    assert!(parse_iostat_data_line(&["1.0", "abc", "3.0"], &one_disk).is_none());
    // Three valid numeric tokens -> one disk, tps = 23.
    let parsed = parse_iostat_data_line(&["1.0", "23", "4.5"], &one_disk)
        .expect("three numeric tokens parse for one disk");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].0, "disk0");
    assert_eq!(parsed[0].1.iops, 23);
}

#[test]
fn iostat_disk_header_detection_distinguishes_names_from_columns() {
    assert!(is_iostat_disk_header_line(&["disk0", "disk1"]));
    assert!(is_iostat_disk_header_line(&["disk12"]));
    // The column-header line is NOT a disk header.
    assert!(!is_iostat_disk_header_line(&["KB/t", "tps", "KB/s"]));
    // A data row is NOT a disk header.
    assert!(!is_iostat_disk_header_line(&["1.0", "23", "4.5"]));
    // An empty token list is not a header.
    assert!(!is_iostat_disk_header_line(&[]));
}

#[test]
fn iostat_excerpt_returns_latest_data_row_per_disk() {
    // Two complete sample blocks; the second block's data row must win.
    let excerpt = "\
    disk0           disk1
    KB/t tps KB/s    KB/t tps KB/s
    12.34  56  7.89    21.00  99  1.50
    disk0           disk1
    KB/t tps KB/s    KB/t tps KB/s
    11.11  50  0.55    33.33 100  3.33
";
    let parsed =
        parse_iostat_excerpt(excerpt).expect("excerpt contains at least one parseable data row");
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed.get("disk0").map(|r| r.iops), Some(50));
    assert_eq!(parsed.get("disk1").map(|r| r.iops), Some(100));
}

#[test]
fn iostat_excerpt_without_data_row_returns_none() {
    // Only a disk-name header and a column-header row: no numeric data
    // row, so no sample can be produced.
    let header_only = "    disk0           disk1\n    KB/t tps KB/s    KB/t tps KB/s\n";
    assert!(parse_iostat_excerpt(header_only).is_none());
    // A data row that appears before any disk-name header is also
    // rejected, because no disk list is known yet.
    let data_before_header = "    12.34  56  7.89\n    disk0\n";
    assert!(parse_iostat_excerpt(data_before_header).is_none());
}

#[test]
fn iostat_excerpt_handles_single_header_multiple_data_rows() {
    // The header is printed once at the top and per-interval data rows
    // follow without repeating it (the common real-iostat shape).
    let excerpt = "\
    disk0       disk1
    KB/t tps KB/s    KB/t tps KB/s
    10.00  20  0.20    30.00  40  1.20
    11.00  25  0.25    31.00  45  1.25
";
    let parsed = parse_iostat_excerpt(excerpt).expect("parses");
    // The second (last) data row wins for each disk.
    assert_eq!(parsed.get("disk0").map(|r| r.iops), Some(25));
    assert_eq!(parsed.get("disk1").map(|r| r.iops), Some(45));
}
