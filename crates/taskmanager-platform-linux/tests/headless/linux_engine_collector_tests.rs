use super::*;
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::smart::DiskSmart;

#[test]
fn apply_smart_copies_availability_with_optional_fields() {
    let smart = DiskSmart {
        availability: taskmanager_core::core::metrics::SmartAvailability::MissingTool,
        state: DeviceState {
            status: taskmanager_core::core::device_state::DeviceStatus::MissingTool,
            last_success_ms: Some(100),
        },
        provider: Some(ProviderId::borrowed("fixture.smart")),
        failure: Some(taskmanager_core::SmartProviderFailureKind::MissingTool),
        temperature_c: None,
        critical_warning: None,
        temp_critical_c: None,
        percent_used: None,
        power_on_hours: None,
        ata_attributes: None,
    };
    let mut disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .smart_availability(taskmanager_core::core::metrics::SmartAvailability::Available)
        .smart_temperature_c(Some(42.0))
        .build();

    apply_smart(&mut disk, &smart);

    assert_eq!(
        disk.smart_availability,
        taskmanager_core::core::metrics::SmartAvailability::MissingTool
    );
    assert_eq!(disk.smart_temperature_c, None);
    assert_eq!(
        disk.smart_provider
            .as_ref()
            .map(|provider| provider.as_str()),
        Some("fixture.smart")
    );
    assert_eq!(
        disk.smart_failure,
        Some(taskmanager_core::SmartProviderFailureKind::MissingTool)
    );
}

#[test]
fn parse_meminfo_lines_converts_kb_to_bytes_and_drops_malformed() {
    // Realistic /proc/meminfo body; values are kB in the file. The line
    // "not a real meminfo line" parses to key="not" val="a" → "a" is not a
    // u64 → silently dropped (no panic, no insertion).
    let body = "\
MemTotal:       16384000 kB
MemFree:          524288 kB
Cached:          2000000 kB
not a real meminfo line
Buffers:            1024 kB
Committed_AS:    8000000 kB
";
    let map = parse_meminfo_lines(body);
    assert_eq!(map.get("MemTotal"), Some(&(16384000 * 1024)));
    assert_eq!(map.get("MemFree"), Some(&(524288 * 1024)));
    assert_eq!(map.get("Cached"), Some(&(2000000 * 1024)));
    assert_eq!(map.get("Buffers"), Some(&(1024 * 1024)));
    assert_eq!(map.get("Committed_AS"), Some(&(8000000 * 1024)));
    // Malformed line dropped.
    assert!(!map.contains_key("not"));
    assert_eq!(map.len(), 5);
}

#[test]
fn parse_meminfo_lines_empty_yields_empty_map() {
    assert!(parse_meminfo_lines("").is_empty());
}

#[test]
fn parse_meminfo_lines_drops_overflowing_values_without_panicking() {
    // A malformed /proc/meminfo whose kB value is near u64::MAX must not
    // panic (debug) or wrap (release): the overflowing key is dropped to
    // typed absence while the remaining lines keep exact byte values.
    let body = "\
MemTotal: 16384 kB
HugeValue: 18446744073709551615 kB
WrapValue: 18014398509481984 kB
AlsoHuge: 18014398509481983 kB
";
    let map = parse_meminfo_lines(body);
    assert_eq!(map.get("MemTotal"), Some(&(16384 * 1024)));
    assert!(!map.contains_key("HugeValue"));
    assert!(!map.contains_key("WrapValue"));
    // The largest value whose kB→bytes conversion still fits u64 survives.
    assert_eq!(map.get("AlsoHuge"), Some(&(18014398509481983 * 1024)));
    assert_eq!(map.len(), 2);
}

#[test]
fn parse_diskstats_observation_extracts_fields_and_drops_short_lines() {
    // Real /proc/diskstats layout (>= 14 whitespace fields per row):
    //   major minor name reads_done reads_merged sectors_read ms_reading
    //   writes_done writes_merged sectors_written ms_writing ios_in_flight
    //   io_time_ms weighted_io_ms [discard/flush tail…]
    // Fields the collector ignores are kept as small fillers; the 5 it reads
    // get distinct sentinel values so a column-index regression shows up.
    let body = "\
   7       0 nvme0n1 100 1 2000 10 200 2 4000 20 0 7000 0
   8       0 sda 5 0 50 1 10 0 100 2 0 9000 0
malformed short line
";
    let map = parse_diskstats_observation(body);

    let nvme = map.get("nvme0n1").expect("nvme0n1 should be parsed");
    assert_eq!(nvme.reads_completed, 100);
    assert_eq!(nvme.sectors_read, 2000);
    assert_eq!(nvme.writes_completed, 200);
    assert_eq!(nvme.sectors_written, 4000);
    assert_eq!(nvme.io_time_ms, 7000);
    assert!(nvme.timestamp.is_none());

    let sda = map.get("sda").expect("sda should be parsed");
    assert_eq!(sda.reads_completed, 5);
    assert_eq!(sda.sectors_read, 50);
    assert_eq!(sda.writes_completed, 10);
    assert_eq!(sda.sectors_written, 100);
    assert_eq!(sda.io_time_ms, 9000);
    assert!(sda.timestamp.is_none());

    // The malformed short line (< 14 fields) is dropped.
    assert!(!map.contains_key("malformed"));
    assert_eq!(map.len(), 2);
}

#[test]
fn parse_diskstats_observation_empty_yields_empty_map() {
    assert!(parse_diskstats_observation("").is_empty());
}
