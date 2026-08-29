use std::fs;
use std::time::Duration;

use super::*;
use crate::engine::collector::sources::parse_diskstats_observation;
use taskmanager_core::ScalarAvailability;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::SmartAvailability;
use taskmanager_core::core::source::SourceOutcome;

fn diskstats(
    name: &str,
    reads: u64,
    sectors_read: u64,
    writes: u64,
    sectors_written: u64,
    io_time_ms: u64,
    weighted_time_ms: u64,
) -> DiskstatsObservation {
    parse_diskstats_observation(&format!(
        "8 0 {name} {reads} 0 {sectors_read} 0 {writes} 0 {sectors_written} 0 0 {io_time_ms} {weighted_time_ms}"
    ))
}

#[test]
fn diskstats_rates_use_one_elapsed_interval_and_whole_device_counters() {
    let start = Instant::now();
    let mut previous = HashMap::from([(
        "future0".to_string(),
        DiskStatsState {
            reads_completed: 10,
            sectors_read: 100,
            writes_completed: 20,
            sectors_written: 200,
            io_time_ms: 300,
            weighted_time_ms: 300,
            timestamp: Some(start),
        },
    )]);
    let current = diskstats("future0", 14, 104, 22, 208, 500, 500);
    let mut metrics = vec![
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .name("/dev/future0".into())
            .build(),
    ];

    apply_diskstats_rates(
        &mut metrics,
        &current,
        &mut previous,
        start + Duration::from_secs(2),
        20,
    );

    assert_eq!(metrics[0].current_read_bytes_per_sec(), Some(1_024));
    assert_eq!(metrics[0].current_write_bytes_per_sec(), Some(2_048));
    assert_eq!(metrics[0].current_iops(), Some(3));
    assert_eq!(metrics[0].current_active_time_pct(), Some(10.0));
    assert!(
        (metrics[0]
            .current_response_time_ms()
            .expect("operations produce latency")
            - (200.0 / 6.0))
            .abs()
            < 0.001
    );
    assert_eq!(
        metrics[0].scalar_observations().iops.last_success_ms(),
        Some(20)
    );
}

#[test]
fn absent_diskstats_row_resets_baseline_and_is_typed_unavailable() {
    let start = Instant::now();
    let mut previous = HashMap::from([(
        "future0".to_string(),
        DiskStatsState {
            timestamp: Some(start),
            ..Default::default()
        },
    )]);
    let mut metrics = vec![
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .name("/dev/future0".into())
            .build(),
    ];

    apply_diskstats_rates(
        &mut metrics,
        &parse_diskstats_observation("not a diskstats row"),
        &mut previous,
        start + Duration::from_secs(1),
        20,
    );

    assert!(
        !previous.contains_key("future0"),
        "a failed interval must not be folded into a later average rate"
    );
    assert_eq!(
        metrics[0].scalar_observations().iops.availability(),
        ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
}

#[test]
fn zero_io_is_current_but_response_time_without_operations_is_unavailable() {
    let start = Instant::now();
    let mut previous = HashMap::from([(
        "future0".to_string(),
        DiskStatsState {
            reads_completed: 10,
            sectors_read: 100,
            writes_completed: 20,
            sectors_written: 200,
            io_time_ms: 300,
            weighted_time_ms: 300,
            timestamp: Some(start),
        },
    )]);
    let current = diskstats("future0", 10, 100, 20, 200, 300, 300);
    let mut metrics = vec![
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .name("/dev/future0".into())
            .build(),
    ];

    apply_diskstats_rates(
        &mut metrics,
        &current,
        &mut previous,
        start + Duration::from_secs(1),
        30,
    );

    assert_eq!(metrics[0].current_read_bytes_per_sec(), Some(0));
    assert_eq!(metrics[0].current_write_bytes_per_sec(), Some(0));
    assert_eq!(metrics[0].current_iops(), Some(0));
    assert_eq!(metrics[0].current_active_time_pct(), Some(0.0));
    assert_eq!(metrics[0].current_response_time_ms(), None);
}

#[test]
fn counter_rollback_is_identity_change_instead_of_zero_activity() {
    let start = Instant::now();
    let mut previous = HashMap::from([(
        "future0".to_string(),
        DiskStatsState {
            reads_completed: 10,
            sectors_read: 100,
            writes_completed: 20,
            sectors_written: 200,
            io_time_ms: 300,
            weighted_time_ms: 300,
            timestamp: Some(start),
        },
    )]);
    let current = diskstats("future0", 1, 10, 2, 20, 30, 30);
    let mut metrics = vec![
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .name("/dev/future0".into())
            .build(),
    ];

    apply_diskstats_rates(
        &mut metrics,
        &current,
        &mut previous,
        start + Duration::from_secs(1),
        40,
    );

    assert_eq!(
        metrics[0]
            .scalar_observations()
            .read_bytes_per_sec
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::IdentityChanged)
    );
    assert_eq!(metrics[0].current_read_bytes_per_sec(), None);
}

#[test]
fn unsupported_smart_enrichment_never_hides_a_discovered_disk() {
    let root = crate::test_support::repo_temp_dir()
        .join(format!("taskmanager-storage-domain-{}", std::process::id()));
    let device = root.join("vda");
    fs::create_dir_all(&device).expect("create isolated virtio fixture");
    fs::write(device.join("size"), "100").expect("write capacity");
    let mut state = DiskCollectionState::new();

    let snapshot = collect_storage_domain(&Disks::new(), &root, &mut state, 2, Instant::now(), 100);

    assert!(snapshot.discovery_is_authoritative());
    assert_eq!(snapshot.discovery().outcome, SourceOutcome::Available);
    assert_eq!(snapshot.value.len(), 1);
    assert_eq!(snapshot.value[0].name, "/dev/vda");
    assert_eq!(
        snapshot.value[0].smart_availability,
        SmartAvailability::Unsupported
    );
    assert!(
        snapshot
            .enrichments
            .iter()
            .any(|source| { source.provider.as_str() == "linux.storage.proc.diskstats" })
    );
    assert!(snapshot.enrichments.iter().any(|source| {
        source.provider.as_str() == "linux.smart.registry"
            && source.outcome == SourceOutcome::Unavailable(FailureKind::Unsupported)
    }));

    fs::remove_dir_all(root).expect("remove isolated virtio fixture");
}

#[test]
fn same_kernel_slot_with_new_stable_identity_resets_rate_generation() {
    let old_identity = DeviceId::new("disk:wwid:old");
    let previous_identities = HashMap::from([("sda".to_string(), old_identity)]);
    let mut stats = HashMap::from([(
        "sda".to_string(),
        DiskStatsState {
            timestamp: Some(Instant::now()),
            ..Default::default()
        },
    )]);
    let metrics = vec![
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .name("/dev/sda".into())
            .device_id("disk:wwid:new".into())
            .build(),
    ];

    reset_changed_identity_rate_baselines(&metrics, &previous_identities, &mut stats);

    assert!(
        !stats.contains_key("sda"),
        "hot-swapping a device in the same kernel slot must start a fresh rate baseline"
    );
}

#[test]
fn non_authoritative_discovery_resets_all_rate_baselines() {
    let baseline = DiskStatsState {
        timestamp: Some(Instant::now()),
        ..Default::default()
    };
    let mut stats = HashMap::from([("sda".to_string(), baseline)]);

    reset_rate_baselines_after_discovery(SourceOutcome::Available, &mut stats);
    assert!(stats.contains_key("sda"));

    reset_rate_baselines_after_discovery(
        SourceOutcome::Unavailable(FailureKind::PermissionDenied),
        &mut stats,
    );
    assert!(
        stats.is_empty(),
        "a discovery gap must not retain counter baselines across an unknown attachment set"
    );
}
