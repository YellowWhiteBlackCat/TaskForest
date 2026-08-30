use taskmanager_core::core::FailureKind;
use taskmanager_core::core::alerts::{
    AlertMetric, InsufficientReason, SuggestedThreshold, SuggestionBasis, SuggestionConfidence,
};
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, DiskMetrics, DiskScalarObservations, GpuMetrics,
    GpuScalarObservations, MemoryScalarObservations, NetworkAdapterType, NetworkMetrics,
    NetworkScalarObservations, NetworkWirelessObservations, OptionalObservation, ScalarObservation,
    SystemSnapshot,
};
use taskmanager_core::core::process::{
    ProcessItem, ProcessMetadataObservation, ProcessMetadataObservations, ProcessOwner,
    ProcessOwnerIdentity, ProcessScalarObservations,
};
use taskmanager_core::core::process_telemetry::{
    ContainerSummary, IsolationKind, ProcessGpuEngineUsage, ProcessGpuEngines,
};
use taskmanager_core::export::{
    ExportExtras, ProcessGpuEnginesEntry, processes_to_csv, processes_to_html, snapshot_to_json,
    snapshot_to_json_with_extras,
};
use taskmanager_test_support::MemoryMetricsFixtureBuilder;

fn snapshot_with_brand(brand: &str) -> SystemSnapshot {
    let mut cpu = CpuMetrics::default();
    cpu.brand = Some(brand.into());
    SystemSnapshot {
        cpu,
        ..Default::default()
    }
}

#[test]
fn csv_escapes_embedded_comma_quote_and_newline() {
    let processes = [
        ProcessItem::new(0, "evil,name.exe"),
        ProcessItem::new(0, r#"say "hi""#),
        ProcessItem::new(0, "line\nbreak"),
    ];

    let csv = processes_to_csv(&processes);

    assert!(csv.contains("\"evil,name.exe\","));
    assert!(csv.contains("\"say \"\"hi\"\"\","));
    assert!(csv.contains("\"line\nbreak\""));
}

#[test]
fn csv_plain_field_is_unquoted() {
    let csv = processes_to_csv(&[ProcessItem::new(0, "plain")]);

    assert!(
        csv.lines()
            .nth(1)
            .is_some_and(|row| row.starts_with("plain,"))
    );
}

#[test]
fn csv_empty_process_list_yields_header_only() {
    assert_eq!(
        processes_to_csv(&[]),
        "Name,PID,CPU%,Memory MB,User,Status,Threads,Disk R,Disk W\n"
    );
}

#[test]
fn process_exports_do_not_turn_typed_unavailable_scalars_into_zero() {
    let mut unavailable = ProcessItem::new(42, "protected.exe");
    unavailable.apply_scalar_observations(
        ProcessScalarObservations::default().transition_failure(FailureKind::PermissionDenied),
    );

    let json: serde_json::Value = serde_json::from_str(&snapshot_to_json(
        &SystemSnapshot::default(),
        &[unavailable.clone()],
    ))
    .expect("process export JSON should parse");
    let process = &json["processes"][0];
    for field in [
        "cpu_usage",
        "memory_bytes",
        "disk_read_bytes",
        "disk_write_bytes",
        "threads",
        "cpu_time_secs",
        "fds",
        "nice",
    ] {
        assert!(process[field].is_null(), "unavailable {field} must be null");
    }

    let csv = processes_to_csv(&[unavailable.clone()]);
    let fields: Vec<_> = csv.lines().nth(1).unwrap_or_default().split(',').collect();
    assert_eq!(fields.get(2), Some(&"—"));
    assert_eq!(fields.get(3), Some(&"—"));
    assert_eq!(fields.get(6), Some(&"—"));
    assert_eq!(fields.get(7), Some(&"—"));
    assert_eq!(fields.get(8), Some(&"—"));

    let html = processes_to_html(&SystemSnapshot::default(), &[unavailable]);
    assert!(html.contains("<td class=\"num\">—</td>"));
    assert!(!html.contains("<td class=\"num\">99</td>"));
    assert!(!html.contains("<td class=\"num\">123</td>"));
}

#[test]
fn snapshot_to_json_round_trips_brand_and_processes() {
    let snapshot = snapshot_with_brand("AMD");
    let processes = [ProcessItem::new(1, "init"), ProcessItem::new(2, "systemd")];

    let value: serde_json::Value = serde_json::from_str(&snapshot_to_json(&snapshot, &processes))
        .expect("snapshot JSON should parse");

    assert_eq!(value["snapshot"]["cpu"]["brand"], "AMD");
    assert_eq!(value["processes"].as_array().map(Vec::len), Some(2));
    assert_eq!(value["processes"][0]["pid"], 1);
    assert_eq!(value["processes"][1]["name"], "systemd");
}

#[test]
fn snapshot_to_json_exports_process_history_gaps_as_null_slots() {
    let mut process = ProcessItem::new(42, "worker");
    process.cpu_history = vec![10.0, f32::NAN, 30.0];

    let value: serde_json::Value =
        serde_json::from_str(&snapshot_to_json(&SystemSnapshot::default(), &[process]))
            .expect("non-finite history must use JSON null");
    assert_eq!(
        value["processes"][0]["cpu_history"],
        serde_json::json!([10.0, null, 30.0])
    );
}

#[test]
fn snapshot_json_keeps_unknown_cpu_observations_distinct_from_zero() {
    let unknown: serde_json::Value =
        serde_json::from_str(&snapshot_to_json(&SystemSnapshot::default(), &[]))
            .expect("snapshot JSON should parse");
    let cpu = &unknown["snapshot"]["cpu"];
    assert!(cpu["brand"].is_null());
    assert!(cpu["physical_cores"].is_null());
    assert!(cpu["logical_cores"].is_null());
    assert!(cpu["l1d_cache_kb"].is_null());
    assert!(cpu["l1i_cache_kb"].is_null());
    assert!(cpu["frequency_mhz"].is_null());
    assert!(cpu["max_freq_mhz"].is_null());
    assert!(cpu["temperature_c"].is_null());

    let measured = SystemSnapshot {
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            frequency_mhz: ScalarObservation::available(0, 10),
            max_frequency_mhz: ScalarObservation::available(0, 10),
            temperature_c: ScalarObservation::available(0.0, 10),
            per_core_frequency_group:
                taskmanager_core::core::metrics::ScalarObservationGroup::partial(
                    vec![
                        taskmanager_core::core::metrics::ScalarObservationSlot::Current(0),
                        taskmanager_core::core::metrics::ScalarObservationSlot::Unavailable(
                            FailureKind::Unsupported,
                        ),
                    ],
                    10,
                    FailureKind::Unsupported,
                ),
            ..Default::default()
        }),
        ..Default::default()
    };
    let measured: serde_json::Value = serde_json::from_str(&snapshot_to_json(&measured, &[]))
        .expect("snapshot JSON should parse");
    let cpu = &measured["snapshot"]["cpu"];
    assert_eq!(cpu["frequency_mhz"].as_u64(), Some(0));
    assert_eq!(cpu["max_freq_mhz"].as_u64(), Some(0));
    assert_eq!(cpu["temperature_c"].as_f64(), Some(0.0));
    assert_eq!(cpu["per_core_freq_mhz"], serde_json::json!([0, null]));
}

#[test]
fn snapshot_json_exports_the_zram_mm_stat_depth_including_memory_used_total() {
    let mib = 1024_u64 * 1024;
    let snapshot = SystemSnapshot {
        memory: MemoryMetricsFixtureBuilder::new()
            .current_swap_total_bytes(4 * mib)
            .current_swap_used_bytes(mib)
            .compressed_swap_used_bytes(mib)
            .compressed_swap_capacity_bytes(4 * mib)
            .compressed_swap_original_bytes(3 * mib)
            .compressed_swap_compressed_bytes(mib)
            .compressed_swap_memory_used_bytes(mib / 2)
            .build(),
        ..Default::default()
    };

    let value: serde_json::Value = serde_json::from_str(&snapshot_to_json(&snapshot, &[]))
        .expect("snapshot JSON should parse");
    let compression = &value["snapshot"]["memory"]["optional_observations"]["compression"];
    for (field, bytes) in [
        ("compressed_swap_used_bytes", mib),
        ("compressed_swap_original_bytes", 3 * mib),
        ("compressed_swap_compressed_bytes", mib),
        // zram `mm_stat` `mem_used_total`: the RAM the store actually
        // consumes — distinct from both the swap-used view and the
        // compressed size, so its export name is pinned too.
        ("compressed_swap_memory_used_bytes", mib / 2),
    ] {
        assert_eq!(
            compression[field]["state"]["state"], "present",
            "{field} must export as a measured present observation"
        );
        assert_eq!(
            compression[field]["state"]["value"].as_u64(),
            Some(bytes),
            "{field} must carry its measured bytes"
        );
    }
}

#[test]
fn snapshot_json_does_not_fabricate_zram_depth_without_a_store() {
    let value: serde_json::Value =
        serde_json::from_str(&snapshot_to_json(&SystemSnapshot::default(), &[]))
            .expect("snapshot JSON should parse");
    let compression = &value["snapshot"]["memory"]["optional_observations"]["compression"];
    for field in [
        "compressed_swap_used_bytes",
        "compressed_swap_original_bytes",
        "compressed_swap_compressed_bytes",
        "compressed_swap_memory_used_bytes",
    ] {
        assert_ne!(
            compression[field]["state"]["state"], "present",
            "{field} must not look measured on a host without a zram store"
        );
    }
}

#[test]
fn snapshot_json_exports_network_value_freshness_and_optional_semantics() {
    let mut network = NetworkMetrics::new("wlan0");
    network.device_id = "net:mac:aa:bb".into();
    network.apply_observations(
        NetworkAdapterType::WiFi,
        NetworkScalarObservations {
            total_rx_bytes: ScalarObservation::available(0, 10),
            total_tx_bytes: ScalarObservation::available(20, 10),
            rx_bytes_per_sec: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            tx_bytes_per_sec: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            utilization_pct: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            link_speed_mbps: ScalarObservation::available(866, 10),
            link_up: ScalarObservation::available(true, 10),
        },
        NetworkWirelessObservations {
            association: OptionalObservation::absent(10),
            ssid: OptionalObservation::absent(10),
            signal_dbm: OptionalObservation::absent(10),
            ..Default::default()
        },
    );
    let snapshot = SystemSnapshot {
        networks: vec![network],
        ..Default::default()
    };

    let value: serde_json::Value = serde_json::from_str(&snapshot_to_json(&snapshot, &[]))
        .expect("snapshot JSON should parse");
    let network = &value["snapshot"]["networks"][0];

    assert_eq!(network["scalar_observations"]["total_rx_bytes"]["value"], 0);
    assert_eq!(
        network["scalar_observations"]["total_rx_bytes"]["availability"]["status"],
        "available"
    );
    assert_eq!(
        network["scalar_observations"]["rx_bytes_per_sec"]["availability"]["status"],
        "unavailable"
    );
    assert_eq!(
        network["wireless_observations"]["association"]["state"]["state"],
        "absent"
    );
    assert_eq!(
        network["wireless_observations"]["association"]["availability"]["status"],
        "available"
    );
}

#[test]
fn snapshot_json_never_reexports_unavailable_legacy_device_scalars_as_zero() {
    let mut disk = DiskMetrics::new("nvme0n1");
    disk.apply_scalar_observations(DiskScalarObservations::unavailable(
        FailureKind::TemporarilyUnavailable,
    ));

    let mut network = NetworkMetrics::new("wlan0");
    network.device_id = "net:mac:02:11:22:33:44:55".into();
    network.apply_observations(
        NetworkAdapterType::WiFi,
        NetworkScalarObservations::unavailable(FailureKind::TemporarilyUnavailable),
        NetworkWirelessObservations::unavailable(FailureKind::TemporarilyUnavailable),
    );

    let mut gpu = GpuMetrics::new("gpu:0", "Fixture GPU");
    gpu.apply_scalar_observations(GpuScalarObservations::unavailable(
        FailureKind::TemporarilyUnavailable,
    ));

    let value: serde_json::Value = serde_json::from_str(&snapshot_to_json(
        &SystemSnapshot {
            disks: vec![disk],
            networks: vec![network],
            gpu: vec![gpu],
            ..Default::default()
        },
        &[],
    ))
    .expect("snapshot JSON should parse");

    assert!(value["snapshot"]["disks"][0]["read_bytes_per_sec"].is_null());
    assert!(value["snapshot"]["networks"][0]["rx_bytes_per_sec"].is_null());
    assert!(value["snapshot"]["gpu"][0]["gpu_usage_pct"].is_null());
    assert_eq!(
        value["snapshot"]["gpu"][0]["scalar_observations"]["utilization_pct"]["availability"]["status"],
        "unavailable"
    );
}

#[test]
fn html_renders_unknown_cpu_and_memory_denominator_as_em_dash() {
    let html = processes_to_html(&SystemSnapshot::default(), &[]);

    assert!(html.contains("<th>CPU</th><td>— &middot;"));
    assert!(html.contains("<th>Memory</th><td>— / — (—)</td>"));
    assert!(html.contains("<th>Swap</th><td>— / — (—)</td>"));
}

#[test]
fn html_does_not_export_legacy_memory_numbers_after_typed_failure() {
    let snapshot = SystemSnapshot {
        memory: MemoryMetricsFixtureBuilder::new()
            .scalar_observations(MemoryScalarObservations::unavailable(
                FailureKind::PermissionDenied,
            ))
            .build(),
        ..Default::default()
    };

    let html = processes_to_html(&snapshot, &[]);

    assert!(html.contains("<th>Memory</th><td>— / — (—)</td>"));
    assert!(html.contains("<th>Swap</th><td>— / — (—)</td>"));
    assert!(!html.contains("0.0 MiB / 0.0 MiB"));
}

#[test]
fn html_does_not_export_legacy_cpu_usage_after_typed_failure() {
    let snapshot = SystemSnapshot {
        cpu: CpuMetrics::from_observations(CpuScalarObservations::unavailable(
            FailureKind::PermissionDenied,
        )),
        ..Default::default()
    };

    let html = processes_to_html(&snapshot, &[]);

    assert!(html.contains("<th>CPU</th><td>— &middot; — global</td>"));
    assert!(!html.contains("88.0% global"));
}

#[test]
fn html_escape_covers_every_significant_char_and_preserves_unicode_through_the_public_seam() {
    // The escape map is asserted through `processes_to_html` (the exported
    // surface), not the private `html_escape` helper: the process-name cell
    // must carry the full significant-char map with multi-byte UTF-8 intact.
    let html = processes_to_html(
        &SystemSnapshot::default(),
        &[ProcessItem::new(0, "<a href=\"x\">&'café 微'</a>")],
    );

    assert!(
        html.contains("&lt;a href=&quot;x&quot;&gt;&amp;&#39;café 微&#39;&lt;/a&gt;"),
        "the process-name cell must carry the complete escape map:\n{html}"
    );
    assert!(!html.contains("<a href="));
}

#[test]
fn html_neutralizes_process_markup() {
    let html = processes_to_html(
        &SystemSnapshot::default(),
        &[ProcessItem::new(0, "<script>alert(1)</script>")],
    );

    assert!(!html.contains("<script>"));
    assert!(html.contains("&lt;script&gt;"));
}

#[test]
fn html_contains_document_structure_and_snapshot_stats() {
    let snapshot = SystemSnapshot {
        cpu: {
            let mut cpu = CpuMetrics::from_observations(CpuScalarObservations {
                global_usage_pct: ScalarObservation::available(42.0, 10),
                ..Default::default()
            });
            cpu.brand = Some("Test CPU".into());
            cpu
        },
        memory: MemoryMetricsFixtureBuilder::new()
            .current_total_bytes(2_048 * 1_024 * 1_024)
            .current_used_bytes(512 * 1_024 * 1_024)
            .current_available_bytes(1_536 * 1_024 * 1_024)
            .build(),
        uptime_secs: 99,
        processes: 3,
        threads: Some(7),
        ..Default::default()
    };
    let mut process = ProcessItem::new(1, "init");
    process.status = "Sleeping".into();
    process.apply_metadata_observations(ProcessMetadataObservations {
        owner: ProcessMetadataObservation::available(
            ProcessOwner {
                identity: ProcessOwnerIdentity::Opaque("root".into()),
                label: None,
            },
            1,
        ),
        executable_path: ProcessMetadataObservation::absent(1),
    });
    let html = processes_to_html(&snapshot, &[process]);

    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<style>"));
    assert!(!html.contains("<link"));
    assert!(!html.contains("src=\""));
    assert!(html.contains("Test CPU"));
    assert!(html.contains("&middot; 42.0% global"));
    assert!(html.contains("<th>Processes</th><td>3</td>"));
    assert!(html.contains("<th>Threads</th><td>7</td>"));
    assert!(html.contains("<th>Uptime</th><td>99s</td>"));
    assert!(html.contains("(25%)"));
    assert!(html.contains("<td>init</td>"));
    assert!(html.contains("<td>root</td>"));
    assert!(html.contains("<td>Sleeping</td>"));
    assert!(html.contains("</html>"));
}

#[test]
fn html_empty_process_list_is_explicit() {
    let html = processes_to_html(&SystemSnapshot::default(), &[]);

    assert!(html.contains("<em>No processes in this snapshot.</em>"));
    assert!(html.contains(">0 process(es)<"));
}

#[test]
fn snapshot_to_json_with_extras_emits_containers_gpu_engines_and_thresholds() {
    let container = ContainerSummary {
        id: "/docker/abc".into(),
        name: "abc".into(),
        runtime: Some(IsolationKind::Docker),
        cgroup_path: "/docker/abc".into(),
        cpu_percentage: ScalarObservation::available(12.5, 1_000),
        memory_bytes: ScalarObservation::available(1_048_576, 1_000),
        member_pids: vec![1234, 1235],
    };
    let engines = ProcessGpuEngines {
        state: DeviceState::healthy(1_000),
        engines: vec![ProcessGpuEngineUsage {
            name: "render".into(),
            usage_pct: ScalarObservation::available(42.0, 1_000),
            engine_time_ns: ScalarObservation::available(8_000_000, 1_000),
            engine_cycles: ScalarObservation::default(),
        }],
    };
    let gpu_entry = ProcessGpuEnginesEntry {
        pid: 1234,
        engines: &engines,
    };
    let suggested = SuggestedThreshold::Suggested {
        metric: AlertMetric::CpuUsagePercent,
        threshold: 87.5,
        hysteresis: 4.375,
        basis: SuggestionBasis::MeanPlusStddevFloorP95,
        sample_count: 42,
        confidence: SuggestionConfidence::High,
    };
    let extras = ExportExtras {
        containers: std::slice::from_ref(&container),
        process_gpu_engines: std::slice::from_ref(&gpu_entry),
        suggested_thresholds: std::slice::from_ref(&suggested),
        hardware: None,
        npu_inventory: None,
    };

    let value: serde_json::Value = serde_json::from_str(&snapshot_to_json_with_extras(
        &SystemSnapshot::default(),
        &[],
        extras,
    ))
    .expect("extras JSON should parse");

    // Container row round-trips with its typed engine/runtime identity.
    assert_eq!(value["containers"][0]["id"], "/docker/abc");
    assert_eq!(value["containers"][0]["runtime"], "docker");
    assert_eq!(value["containers"][0]["cpu_percentage"]["value"], 12.5);
    assert_eq!(
        value["containers"][0]["member_pids"],
        serde_json::json!([1234, 1235])
    );

    // Per-process GPU engine entry is keyed by pid and nests the typed engines.
    assert_eq!(value["process_gpu_engines"][0]["pid"], 1234);
    assert_eq!(
        value["process_gpu_engines"][0]["engines"]["engines"][0]["name"],
        "render"
    );
    assert_eq!(
        value["process_gpu_engines"][0]["engines"]["engines"][0]["usage_pct"]["value"],
        42.0
    );

    // Suggested threshold carries its derivation, never a bare number.
    let suggested_node = &value["suggested_thresholds"][0]["Suggested"];
    assert!(
        suggested_node.get("metric").is_some(),
        "Suggested variant must be tagged"
    );
    assert_eq!(suggested_node["metric"], "cpu_usage_percent");
    assert_eq!(suggested_node["threshold"], 87.5);
    assert_eq!(suggested_node["confidence"], "High");
}

#[test]
fn snapshot_to_json_emits_empty_extras_arrays_when_no_extras_supplied() {
    let value: serde_json::Value =
        serde_json::from_str(&snapshot_to_json(&SystemSnapshot::default(), &[]))
            .expect("snapshot JSON should parse");

    // The three additive keys are always present as honest empty arrays — never
    // absent (which a consumer might misread as "feature missing") and never
    // fabricated with a sentinel row.
    for key in ["containers", "process_gpu_engines", "suggested_thresholds"] {
        assert!(
            value[key].is_array(),
            "{key} must always serialize as an array"
        );
        assert_eq!(
            value[key].as_array().map(Vec::len),
            Some(0),
            "{key} must be empty when no extras are supplied"
        );
    }
}

#[test]
fn insufficient_threshold_serializes_reason_without_a_fabricated_value() {
    let insufficient = SuggestedThreshold::Insufficient {
        sample_count: 3,
        required: 20,
        reason: InsufficientReason::TooFewSamples,
    };
    let extras = ExportExtras {
        containers: &[],
        process_gpu_engines: &[],
        suggested_thresholds: std::slice::from_ref(&insufficient),
        hardware: None,
        npu_inventory: None,
    };

    let value: serde_json::Value = serde_json::from_str(&snapshot_to_json_with_extras(
        &SystemSnapshot::default(),
        &[],
        extras,
    ))
    .expect("insufficient JSON should parse");
    let entry = &value["suggested_thresholds"][0]["Insufficient"];

    // The Insufficient variant is a typed discriminator carrying its reason,
    // sample_count and required floor — and must NOT carry a fabricated
    // `threshold` field that a consumer could mistake for a proposal.
    assert!(
        entry.get("reason").is_some(),
        "Insufficient variant must be tagged"
    );
    assert_eq!(entry["reason"], "TooFewSamples");
    assert_eq!(entry["sample_count"], 3);
    assert_eq!(entry["required"], 20);
    assert!(
        entry.get("threshold").is_none(),
        "an Insufficient suggestion must never carry a fabricated threshold"
    );
}

#[test]
fn hardware_and_npu_extras_serialize_when_supplied_and_omit_when_absent() {
    use taskmanager_core::core::hardware::{ComputeTopology, HardwareInfo};
    use taskmanager_core::core::npu::NpuDevice;
    use taskmanager_core::core::{
        CpuInstructionFeature, DeviceId, NpuInventorySnapshot, ScalarObservation,
    };

    let hardware = HardwareInfo::from_fragments(
        Default::default(),
        Default::default(),
        ComputeTopology {
            cpu_brand: Some("Intel Core Ultra 7 358H".into()),
            instruction_features: vec![
                CpuInstructionFeature::Avx2,
                CpuInstructionFeature::AvxVnni,
                CpuInstructionFeature::AmxInt8,
            ],
            ..ComputeTopology::default()
        },
        Default::default(),
    );
    let npu = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("accel0"),
            driver: Some("intel_vpu".into()),
            ..NpuDevice::default()
        }],
        42,
    );

    let extras = ExportExtras {
        containers: &[],
        process_gpu_engines: &[],
        suggested_thresholds: &[],
        hardware: Some(&hardware),
        npu_inventory: Some(&npu),
    };
    let value: serde_json::Value = serde_json::from_str(&snapshot_to_json_with_extras(
        &SystemSnapshot::default(),
        &[],
        extras,
    ))
    .expect("hardware+npu envelope must parse");
    assert_eq!(value["hardware"]["cpu_brand"], "Intel Core Ultra 7 358H");
    assert_eq!(
        value["hardware"]["instruction_features"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );
    assert_eq!(value["npu_inventory"]["devices"][0]["device_id"], "accel0");
    assert_eq!(value["npu_inventory"]["devices"][0]["driver"], "intel_vpu");
    // Typed-unavailable utilization serializes its failure state, never 0.
    assert!(value["npu_inventory"]["devices"][0]["utilization_pct"]["value"].is_null());

    // Absent extras omit the keys entirely (stable additive shape).
    let bare: serde_json::Value =
        serde_json::from_str(&snapshot_to_json(&SystemSnapshot::default(), &[]))
            .expect("bare envelope must parse");
    assert!(bare.get("hardware").is_none());
    assert!(bare.get("npu_inventory").is_none());
    let _ = ScalarObservation::<f32>::default();
}
