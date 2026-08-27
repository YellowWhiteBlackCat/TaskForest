use super::*;

#[test]
fn typed_gpu_zero_is_current_and_explicit_failure_never_uses_legacy_value() {
    let mut gpu = GpuMetrics::default();
    gpu.apply_scalar_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(0.0, 10),
        temperature_c: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        ..Default::default()
    });

    assert_eq!(gpu.current_utilization_pct(), Some(0.0));
    assert_eq!(gpu.current_temperature_c(), None);
}

#[test]
fn retained_gpu_value_is_stale_with_original_last_success() {
    let previous = GpuScalarObservations {
        power_w: ScalarObservation::available(125.0, 10),
        ..Default::default()
    };
    let current =
        GpuScalarObservations::unavailable(FailureKind::PermissionDenied).retain_previous(previous);

    assert_eq!(
        current.power_w.availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(current.power_w.current_value(), None);
    assert_eq!(current.power_w.last_known_value(), Some(&125.0));
    assert_eq!(current.power_w.last_success_ms(), Some(10));
}

#[test]
fn dedicated_vram_zero_is_a_measured_idle_board_not_unavailable() {
    let mut gpu = GpuMetrics::default();
    gpu.apply_scalar_observations(GpuScalarObservations {
        dedicated_vram_used_bytes: ScalarObservation::available(0, 10),
        dedicated_vram_total_bytes: ScalarObservation::available(8 << 30, 10),
        ..Default::default()
    });

    assert_eq!(gpu.current_dedicated_vram_used_bytes(), Some(0));
    assert_eq!(gpu.current_dedicated_vram_total_bytes(), Some(8 << 30));
}

#[test]
fn missing_dedicated_vram_is_unavailable_not_a_legacy_zero() {
    let mut gpu = GpuMetrics::default();
    gpu.apply_scalar_observations(GpuScalarObservations::unavailable(
        FailureKind::ProviderFault,
    ));

    assert_eq!(gpu.current_dedicated_vram_used_bytes(), None);
    assert_eq!(gpu.current_dedicated_vram_total_bytes(), None);
    // Shared aperture read points never masquerade as dedicated memory.
    assert_eq!(gpu.current_shared_vram_used_bytes(), None);
}

#[test]
fn shared_vram_observations_flow_to_their_own_accessors() {
    let mut gpu = GpuMetrics::default();
    gpu.apply_scalar_observations(GpuScalarObservations {
        shared_vram_used_bytes: ScalarObservation::available(512 << 20, 10),
        shared_vram_total_bytes: ScalarObservation::available(1 << 30, 10),
        ..Default::default()
    });

    assert_eq!(gpu.current_shared_vram_used_bytes(), Some(512 << 20));
    assert_eq!(gpu.current_shared_vram_total_bytes(), Some(1 << 30));
    // Dedicated projections stay empty on a unified-memory device.
    assert_eq!(gpu.current_dedicated_vram_total_bytes(), None);
}

#[test]
fn graphics_api_facts_roundtrip_as_optional_typed_identity() {
    let mut gpu = GpuMetrics::new("gpu:pci:0000:01:00.0", "Fixture GPU");
    gpu.graphics_api = Some(GpuGraphicsApi {
        opengl_version: Some("4.6".into()),
        vulkan_version: Some("1.4.354".into()),
    });

    let value = serde_json::to_value(&gpu).expect("GPU graphics API facts serialize");
    assert_eq!(
        value["graphics_api"]["opengl_version"],
        serde_json::json!("4.6")
    );
    assert_eq!(
        value["graphics_api"]["vulkan_version"],
        serde_json::json!("1.4.354")
    );

    let decoded: GpuMetrics = serde_json::from_value(value).expect("GPU graphics API facts decode");
    assert_eq!(decoded.graphics_api, gpu.graphics_api);
}

#[test]
fn current_envelope_hydrates_legacy_values_with_exact_field_provenance() {
    let value = serde_json::json!({
        "device_id": "gpu:pci:0000:01:00.0",
        "device_generation": 1,
        "device_state": { "status": "healthy", "last_success_ms": 42 },
        "provenance": [
            { "field": "utilization", "provider": "fixture.gpu" },
            { "field": "memory", "provider": "fixture.gpu" }
        ],
        "brand": "Fixture GPU",
        "utilization_pct": 0.0,
        "memory_used_bytes": 0,
        "memory_total_bytes": 1024
    });

    let gpu: GpuMetrics = serde_json::from_value(value).expect("trusted legacy GPU row");
    assert_eq!(gpu.current_utilization_pct(), Some(0.0));
    assert_eq!(gpu.current_memory_used_bytes(), Some(0));
    assert_eq!(gpu.current_memory_total_bytes(), Some(1024));
    assert_eq!(
        gpu.scalar_observations().utilization_pct.last_success_ms(),
        Some(42)
    );
}

#[test]
fn previous_derived_serde_envelope_hydrates_legacy_scalars() {
    let value = serde_json::json!({
        "device_id": "gpu:pci:0000:01:00.0",
        "device_generation": 0,
        "device_state": { "status": "unsupported", "last_success_ms": null },
        "provenance": [],
        "scalar_observations": {},
        "brand": "Fixture GPU",
        "gpu_usage_pct": 37.0,
        "utilization_pct": 37.0,
        "vram_used_bytes": 1024,
        "vram_total_bytes": 4096,
        "dedicated_vram_used_bytes": 1024,
        "dedicated_vram_total_bytes": 4096,
        "temp_celsius": 61.0,
        "temperature_c": 61.0,
        "gpu_freq_mhz": 1800,
        "gpu_throttle_reason": "",
        "throttle_reasons": []
    });

    let gpu: GpuMetrics = serde_json::from_value(value).expect("previous GPU envelope");
    assert_eq!(gpu.current_utilization_pct(), Some(37.0));
    assert_eq!(gpu.current_dedicated_vram_used_bytes(), Some(1024));
    assert_eq!(gpu.current_dedicated_vram_total_bytes(), Some(4096));
    assert_eq!(gpu.current_temperature_c(), Some(61.0));
    assert_eq!(gpu.current_frequency_mhz(), Some(1800));
    assert_eq!(gpu.current_throttle_reasons(), None);
}

#[test]
fn schema_v1_payload_without_lifecycle_or_provenance_still_imports() {
    let value = serde_json::json!({
        "device_id": "gpu:pci:0000:01:00.0",
        "brand": "Fixture GPU",
        "utilization_pct": 21.0,
        "memory_used_bytes": 0,
        "memory_total_bytes": 2048
    });
    let gpu: GpuMetrics = serde_json::from_value(value).expect("schema-v1 GPU row");
    assert_eq!(gpu.current_utilization_pct(), Some(21.0));
    assert_eq!(gpu.current_memory_used_bytes(), Some(0));
    assert_eq!(gpu.current_memory_total_bytes(), Some(2048));

    let unidentified: GpuMetrics = serde_json::from_value(serde_json::json!({
        "brand": "Fixture GPU",
        "utilization_pct": 21.0
    }))
    .expect("unidentified legacy row remains readable");
    assert_eq!(unidentified.current_utilization_pct(), None);
}

#[test]
fn legacy_numeric_zero_sentinels_do_not_become_measured_values() {
    let value = serde_json::json!({
        "device_id": "gpu:pci:0000:01:00.0",
        "device_generation": 1,
        "device_state": { "status": "healthy", "last_success_ms": 42 },
        "provenance": [
            { "field": "utilization", "provider": "fixture.gpu" },
            { "field": "dedicated_vram", "provider": "fixture.gpu" },
            { "field": "frequency", "provider": "fixture.gpu" }
        ],
        "brand": "Fixture GPU",
        "gpu_usage_pct": 0.0,
        "vram_used_bytes": 0,
        "vram_total_bytes": 0,
        "gpu_freq_mhz": 0
    });

    let gpu: GpuMetrics = serde_json::from_value(value).expect("legacy GPU row");
    assert_eq!(gpu.current_utilization_pct(), None);
    assert_eq!(gpu.current_dedicated_vram_used_bytes(), None);
    assert_eq!(gpu.current_dedicated_vram_total_bytes(), None);
    assert_eq!(gpu.current_frequency_mhz(), None);
}

#[test]
fn typed_gpu_truth_wins_over_conflicting_legacy_keys() {
    let value = serde_json::json!({
        "device_id": "gpu:pci:0000:01:00.0",
        "device_generation": 1,
        "device_state": { "status": "healthy", "last_success_ms": 42 },
        "provenance": [{ "field": "utilization", "provider": "fixture.gpu" }],
        "brand": "Fixture GPU",
        "utilization_pct": 91.0,
        "scalar_observations": {
            "utilization_pct": {
                "value": 12.0,
                "availability": { "status": "available" },
                "last_success_ms": 44
            }
        }
    });

    let gpu: GpuMetrics = serde_json::from_value(value).expect("mixed GPU row");
    assert_eq!(gpu.current_utilization_pct(), Some(12.0));
}

#[test]
fn gpu_failure_json_omits_legacy_success_keys() {
    let mut gpu = GpuMetrics {
        device_id: "gpu:pci:0000:01:00.0".into(),
        device_generation: DeviceGeneration::INITIAL,
        device_state: DeviceState::healthy(42),
        brand: "Fixture GPU".into(),
        ..Default::default()
    };
    gpu.apply_scalar_observations(GpuScalarObservations::unavailable(
        FailureKind::PermissionDenied,
    ));
    gpu.apply_throttle_observation(ScalarObservation::unavailable(
        FailureKind::PermissionDenied,
    ));

    let value = serde_json::to_value(gpu).expect("failed GPU row serializes");
    for key in [
        "gpu_usage_pct",
        "utilization_pct",
        "vram_used_bytes",
        "vram_total_bytes",
        "temp_celsius",
        "temperature_c",
        "gpu_power_w",
        "gpu_freq_mhz",
        "gpu_throttle_reason",
        "throttle_reasons",
    ] {
        assert!(value.get(key).is_none(), "unexpected legacy key {key}");
    }
}

#[test]
fn throttle_availability_distinguishes_confirmed_empty_from_failure() {
    let mut gpu = GpuMetrics::default();
    gpu.apply_throttle_observation(ScalarObservation::available(Vec::new(), 10));
    assert_eq!(gpu.current_throttle_reasons(), Some([].as_slice()));
    assert_eq!(gpu.current_throttle_reason_text().as_deref(), Some(""));

    gpu.apply_throttle_observation(ScalarObservation::unavailable(
        FailureKind::TemporarilyUnavailable,
    ));
    assert_eq!(gpu.current_throttle_reasons(), None);
    assert_eq!(gpu.current_throttle_reason_text(), None);
}

#[test]
fn retained_vram_observation_keeps_its_original_last_success() {
    let previous = GpuScalarObservations {
        dedicated_vram_used_bytes: ScalarObservation::available(3 << 30, 10),
        ..Default::default()
    };
    let current =
        GpuScalarObservations::unavailable(FailureKind::TimedOut).retain_previous(previous);

    assert_eq!(
        current.dedicated_vram_used_bytes.availability(),
        ScalarAvailability::Stale(FailureKind::TimedOut)
    );
    assert_eq!(current.dedicated_vram_used_bytes.current_value(), None);
    assert_eq!(
        current.dedicated_vram_used_bytes.last_known_value(),
        Some(&(3 << 30))
    );
    assert_eq!(
        current.dedicated_vram_used_bytes.last_success_ms(),
        Some(10)
    );
}

#[test]
fn engine_kind_maps_only_proven_display_labels() {
    assert_eq!(
        GpuEngineKind::from_display_name("Render/3D"),
        GpuEngineKind::Render
    );
    assert_eq!(
        GpuEngineKind::from_display_name("Graphics (3D)"),
        GpuEngineKind::Render
    );
    assert_eq!(
        GpuEngineKind::from_display_name("Compute"),
        GpuEngineKind::Compute
    );
    assert_eq!(
        GpuEngineKind::from_display_name("Memory (Copy)"),
        GpuEngineKind::Copy
    );
    assert_eq!(
        GpuEngineKind::from_display_name("Video Decode"),
        GpuEngineKind::VideoDecode
    );
    assert_eq!(
        GpuEngineKind::from_display_name("Video Encode"),
        GpuEngineKind::VideoEncode
    );
    assert_eq!(
        GpuEngineKind::from_display_name("FUTURE MEDIA"),
        GpuEngineKind::Unknown
    );
}

#[test]
fn engine_history_point_filters_non_finite_and_duplicate_names() {
    let point = GpuEngineMetricPoint::from_metrics(&GpuMetrics {
        device_id: "gpu:pci:0000:00:02.0".into(),
        engines: vec![
            GpuEngine {
                name: "Video Decode".into(),
                kind: GpuEngineKind::VideoDecode,
                usage_pct: 0.0,
            },
            GpuEngine {
                name: "Render/3D".into(),
                kind: GpuEngineKind::Render,
                usage_pct: f32::NAN,
            },
            GpuEngine {
                name: "Video Decode".into(),
                kind: GpuEngineKind::VideoDecode,
                usage_pct: 0.0,
            },
        ],
        ..Default::default()
    })
    .expect("finite named engine should be retained");

    assert_eq!(point.engines.len(), 1);
    assert_eq!(point.engines[0].name, "Video Decode");
    assert_eq!(point.engines[0].kind, GpuEngineKind::VideoDecode);
    assert_eq!(point.engines[0].utilization_pct, 0.0);
    assert!(GpuEngineMetricPoint::from_metrics(&GpuMetrics::default()).is_none());
}

#[test]
fn engine_history_rejects_same_name_with_conflicting_values() {
    let metrics = GpuMetrics {
        device_id: "gpu:pci:0000:00:02.0".into(),
        engines: vec![
            GpuEngine {
                name: "Render/3D".into(),
                kind: GpuEngineKind::Render,
                usage_pct: 1.0,
            },
            GpuEngine {
                name: "Render/3D".into(),
                kind: GpuEngineKind::Render,
                usage_pct: 2.0,
            },
        ],
        ..Default::default()
    };

    assert!(GpuEngineMetricPoint::from_metrics(&metrics).is_none());
}

#[test]
fn engine_history_rejects_same_name_with_conflicting_semantics() {
    let metrics = GpuMetrics {
        device_id: "gpu:pci:0000:00:02.0".into(),
        engines: vec![
            GpuEngine {
                name: "Video Decode".into(),
                kind: GpuEngineKind::VideoDecode,
                usage_pct: 1.0,
            },
            GpuEngine {
                name: "Video Decode".into(),
                kind: GpuEngineKind::VideoEncode,
                usage_pct: 2.0,
            },
        ],
        ..Default::default()
    };

    assert!(GpuEngineMetricPoint::from_metrics(&metrics).is_none());
}

#[test]
fn legacy_engine_wire_defaults_to_unknown_kind() {
    let legacy: GpuEngine = serde_json::from_str(r#"{"name":"Future Media","usage_pct":12.0}"#)
        .expect("legacy engine payload should remain readable");
    assert_eq!(legacy.kind, GpuEngineKind::Unknown);

    let current = serde_json::to_value(GpuEngine {
        name: "Video Decode".into(),
        kind: GpuEngineKind::VideoDecode,
        usage_pct: 12.0,
    })
    .expect("typed engine payload should serialize");
    assert_eq!(current["kind"], "video_decode");
}
