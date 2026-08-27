use super::*;
use std::collections::HashMap;

#[test]
fn sysctl_optional_keys_are_unique_and_never_cover_x86_only_features() {
    let mut seen = std::collections::HashSet::new();
    for feature in taskmanager_core::CpuInstructionFeature::ALL {
        if let Some(key) = sysctl_optional_key(*feature) {
            assert!(seen.insert(key), "duplicate sysctl key {key}");
        }
    }
    assert_eq!(
        sysctl_optional_key(taskmanager_core::CpuInstructionFeature::Sse41),
        None,
        "macOS has no hw.optional key for SSE4.1; it must stay unreported"
    );
    assert_eq!(
        sysctl_optional_key(taskmanager_core::CpuInstructionFeature::Neon),
        Some("hw.optional.neon")
    );
}

#[test]
fn memory_pressure_rate_is_none_without_a_previous_sample() {
    assert_eq!(used_rate_mib_per_sec(None, 100, 1_000), None);
}

#[test]
fn memory_pressure_rate_is_none_when_no_time_elapsed() {
    // Same timestamp as the previous sample — would divide by zero.
    assert_eq!(used_rate_mib_per_sec(Some((100, 5_000)), 200, 5_000), None);
}

#[test]
fn memory_pressure_rate_computes_growth_in_mib_per_second() {
    const MIB: u64 = 1024 * 1024;
    // +3 MiB of used memory over 2 seconds -> 1.5 MiB/s.
    assert_eq!(
        used_rate_mib_per_sec(Some((100 * MIB, 0)), 103 * MIB, 2_000),
        Some(1.5_f32),
    );
}

#[test]
fn memory_pressure_rate_is_signed_for_freed_memory() {
    const MIB: u64 = 1024 * 1024;
    // -2 MiB over 1 second -> -2.0 MiB/s (memory released).
    assert_eq!(
        used_rate_mib_per_sec(Some((100 * MIB, 0)), 98 * MIB, 1_000),
        Some(-2.0_f32),
    );
}

#[test]
fn container_provider_returns_typed_unavailable_rollup_not_err() {
    // cgroup-v2 is Linux-only: macOS must surface a typed-unavailable
    // rollup (Unsupported) that rides the snapshot lane, NOT an `Err` that
    // would route to `batch.failures` and leave the page at its
    // `empty_healthy` default ("no containers detected" on a host where no
    // host-side container view exists at all).
    let mut provider = MacContainerRollupProvider;
    let rollup = provider
        .refresh(1_000)
        .expect("typed unavailable rollup, not an Err failure");
    assert_eq!(rollup.state.status, DeviceStatus::Unsupported);
    assert!(
        rollup.containers.is_empty(),
        "an unsupported rollup never carries fabricated rows"
    );
}

#[test]
fn parse_hardware_json_maps_all_firmware_fields() {
    // Verbatim-shaped `system_profiler SPHardwareDataType -json` excerpt.
    let body = br#"{
  "SPHardwareDataType": [
    {
      "_name": "hardware overview",
      "boot_rom_version": "10151.61.4",
      "chip_type": "Apple M1 Pro",
      "machine_model": "MacBookPro18,3",
      "machine_name": "MacBook Pro",
      "physical_memory": "16 GB",
      "platform_UUID": "DEADCAFE-0000-0000-0000-000000000001"
    }
  ]
}"#;
    let facts = parse_hardware_json(body);
    assert_eq!(facts.machine_model.as_deref(), Some("MacBookPro18,3"));
    assert_eq!(facts.machine_name.as_deref(), Some("MacBook Pro"));
    assert_eq!(facts.boot_rom_version.as_deref(), Some("10151.61.4"));
    assert_eq!(facts.chip_type.as_deref(), Some("Apple M1 Pro"));
}

#[test]
fn parse_hardware_json_is_all_none_when_body_is_unparsable_or_missing_the_array() {
    // A missing system_profiler (Linux CI) or a truncated body never
    // fabricates firmware facts.
    assert_eq!(parse_hardware_json(b"not json"), HardwareFacts::default());
    assert_eq!(
        parse_hardware_json(br#"{"SPHardwareDataType": []}"#),
        HardwareFacts::default(),
    );
}

#[test]
fn host_threads_sum_from_non_empty_process_facts_cache() {
    // The aggregate host thread count is the sum of every per-process
    // thcount in the injected cache; a process whose thcount is None
    // (column missing for that row) is excluded from the sum, not counted
    // as zero. A fresh cache (just constructed) is reused by `fresh`
    // without shelling out.
    let mut provider = MacHostTelemetryProvider::new();
    provider.process_facts = ProcessFactsCache::with_map(
        HashMap::from([
            (1, (Some(0), Some(3))),
            (2, (Some(0), Some(4))),
            (3, (Some(0), None)),
        ]),
        Instant::now(),
    );
    let observation = provider
        .refresh(1_000)
        .expect("host observation must refresh");
    let facts = observation
        .current_value()
        .expect("host facts must be current");
    assert_eq!(
        facts.threads,
        ScalarObservation::available(7, 1_000),
        "host threads must equal the sum of the contributed thcount values"
    );
}

#[test]
fn host_threads_stay_unavailable_with_an_empty_process_facts_cache() {
    // An empty cache (ps absent on Linux CI, or no thcount column)
    // keeps the host thread scalar honestly Unsupported — never a
    // fabricated zero.
    let mut provider = MacHostTelemetryProvider::new();
    provider.process_facts = ProcessFactsCache::with_map(HashMap::new(), Instant::now());
    let observation = provider
        .refresh(1_000)
        .expect("host observation must refresh");
    let facts = observation
        .current_value()
        .expect("host facts must be current");
    assert_eq!(
        facts.threads,
        ScalarObservation::unavailable(FailureKind::Unsupported)
    );
}
