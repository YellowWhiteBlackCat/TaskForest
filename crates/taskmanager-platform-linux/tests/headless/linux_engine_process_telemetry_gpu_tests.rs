use super::*;

const FDINFO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/drm_fdinfo.txt"
));

#[test]
fn drm_fixture_maps_resident_memory_and_engines() {
    let counter = parse_drm_fdinfo(FDINFO).unwrap();
    assert_eq!(counter.device_id, "gpu:pci:0000:03:00.0");
    assert_eq!(counter.client_id, Some(7));
    assert_eq!(counter.memory_bytes, Some((512 + 128) * 1024));
    assert_eq!(counter.engine_time_ns, Some(25_000_000));
}

#[test]
fn fdinfo_without_stable_device_key_is_not_attributed_by_driver_or_index() {
    assert!(parse_drm_fdinfo("drm-driver: xe\ndrm-engine-render: 10 ns\n").is_none());
}

#[test]
fn rate_needs_two_samples_and_resets_on_counter_rollback() {
    let identity = ProcessIdentity {
        pid: 9,
        start_token: 10,
    };
    let sample = |counter| RawGpuSnapshot {
        state: DeviceState::healthy(1),
        counters: vec![RawGpuCounter {
            device_id: "gpu:pci:test".into(),
            client_id: None,
            memory_bytes: None,
            engine_time_ns: Some(counter),
        }],
    };
    let mut tracker = ProcessGpuRateTracker::default();
    assert_eq!(
        tracker.observe(identity, 1_000, sample(10)).devices[0].utilization_pct,
        None
    );
    assert_eq!(
        tracker
            .observe(identity, 2_000, sample(500_000_010))
            .devices[0]
            .utilization_pct,
        Some(50.0)
    );
    assert_eq!(
        tracker.observe(identity, 3_000, sample(1)).devices[0].utilization_pct,
        None
    );
}

#[test]
fn nvml_memory_merges_by_stable_pci_without_double_counting_drm() {
    let baseline = RawGpuSnapshot {
        state: DeviceState::healthy(100),
        counters: vec![RawGpuCounter {
            device_id: "gpu:pci:0000:03:00.0".into(),
            client_id: Some(7),
            memory_bytes: Some(512),
            engine_time_ns: Some(1_000),
        }],
    };
    let enrichment = RawGpuSnapshot {
        state: DeviceState::healthy(100),
        counters: vec![RawGpuCounter {
            device_id: "gpu:pci:0000:03:00.0".into(),
            client_id: None,
            memory_bytes: Some(768),
            engine_time_ns: None,
        }],
    };

    let merged = merge_gpu_enrichment(baseline, enrichment);

    assert_eq!(merged.counters.len(), 1);
    assert_eq!(merged.counters[0].memory_bytes, Some(768));
    assert_eq!(merged.counters[0].engine_time_ns, Some(1_000));
}

#[test]
fn multi_gpu_enrichment_is_deterministic_and_never_duplicates_matching_pci_devices() {
    let counter = |device_id: &str, memory_bytes, engine_time_ns| RawGpuCounter {
        device_id: device_id.to_string(),
        client_id: None,
        memory_bytes,
        engine_time_ns,
    };
    let baseline = RawGpuSnapshot {
        state: DeviceState::healthy(100),
        counters: vec![
            counter("gpu:pci:0000:04:00.0", Some(400), Some(4_000)),
            counter("gpu:pci:0000:03:00.0", Some(300), Some(3_000)),
        ],
    };
    let enrichment = RawGpuSnapshot {
        state: DeviceState::healthy(100),
        counters: vec![
            counter("gpu:pci:0000:03:00.0", Some(350), None),
            counter("gpu:pci:0000:04:00.0", Some(450), None),
        ],
    };

    let merged = merge_gpu_enrichment(baseline, enrichment);

    assert_eq!(merged.counters.len(), 2);
    assert_eq!(merged.counters[0].device_id, "gpu:pci:0000:03:00.0");
    assert_eq!(merged.counters[0].memory_bytes, Some(350));
    assert_eq!(merged.counters[0].engine_time_ns, Some(3_000));
    assert_eq!(merged.counters[1].device_id, "gpu:pci:0000:04:00.0");
    assert_eq!(merged.counters[1].memory_bytes, Some(450));
    assert_eq!(merged.counters[1].engine_time_ns, Some(4_000));
}

#[test]
fn failed_optional_enrichment_does_not_poison_healthy_drm_baseline() {
    let baseline = RawGpuSnapshot {
        state: DeviceState::healthy(100),
        counters: Vec::new(),
    };
    let unavailable = RawGpuSnapshot {
        state: state_for_status(DeviceStatus::MissingTool, 100),
        counters: Vec::new(),
    };

    assert_eq!(
        merge_gpu_enrichment(baseline, unavailable).state.status,
        DeviceStatus::Healthy
    );
}

/// The live-pid prune contract: baselines for pids that left the authoritative
/// set are dropped (a re-observation re-seeds), while live pids — including
/// other open insight targets — keep theirs.
#[test]
fn rate_tracker_prunes_exited_pids_against_the_live_set() {
    let live = ProcessIdentity {
        pid: 7,
        start_token: 70,
    };
    let exited = ProcessIdentity {
        pid: 8,
        start_token: 80,
    };
    let sample = |counter| RawGpuSnapshot {
        state: DeviceState::healthy(1),
        counters: vec![RawGpuCounter {
            device_id: "gpu:pci:test".into(),
            client_id: None,
            memory_bytes: None,
            engine_time_ns: Some(counter),
        }],
    };
    let mut tracker = ProcessGpuRateTracker::default();
    tracker.observe(live, 1_000, sample(10));
    tracker.observe(exited, 1_000, sample(10));

    tracker.retain_live_pids(&HashSet::from([7]));

    let kept = tracker.observe(live, 2_000, sample(500_000_010));
    assert_eq!(kept.devices[0].utilization_pct, Some(50.0));
    let reseeded = tracker.observe(exited, 2_000, sample(500_000_010));
    assert_eq!(
        reseeded.devices[0].utilization_pct, None,
        "a pruned pid must re-seed, not rate-convert off its dead baseline"
    );
}
