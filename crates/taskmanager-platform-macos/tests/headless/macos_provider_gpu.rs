use super::*;

#[test]
fn profiler_failure_is_unavailable_and_preserves_cause() {
    let observation = gpu_observation_from_profiler(Err(FailureKind::MissingDependency), 1);
    assert!(observation.current_value().is_none());
    assert!(observation.sources().iter().any(|source| {
        source.outcome == SourceOutcome::Unavailable(FailureKind::MissingDependency)
    }));
}

#[test]
fn successful_empty_profiler_inventory_is_authoritative_empty() {
    let adapters = parse_system_profiler_gpus(br#"{"SPDisplaysDataType":[]}"#)
        .expect("valid empty profiler inventory");
    let observation = gpu_observation_from_profiler(Ok(adapters), 2);

    assert!(
        observation
            .current_value()
            .expect("successful empty inventory remains current")
            .is_empty()
    );
    assert_eq!(observation.sources().len(), 1);
    assert_eq!(observation.sources()[0].outcome, SourceOutcome::Empty);
    assert_eq!(observation.sources()[0].item_count, 0);
}

#[test]
fn malformed_or_semantically_incomplete_profiler_output_is_not_empty() {
    assert!(matches!(
        parse_system_profiler_gpus(b"not-json"),
        Err(FailureKind::ProviderFault)
    ));
    assert!(matches!(
        parse_system_profiler_gpus(br#"{}"#),
        Err(FailureKind::ProviderFault)
    ));
}

#[test]
fn memory_string_parses_iec_units_to_bytes() {
    assert_eq!(parse_memory_string("8 GB"), Some(8 * 1024 * 1024 * 1024));
    assert_eq!(parse_memory_string("16384 MB"), Some(16_384 * 1024 * 1024));
    assert_eq!(parse_memory_string("2048 KB"), Some(2048 * 1024));
    assert_eq!(parse_memory_string("0 B"), Some(0));
    // Apple Silicon parts publish no vram field; absent/unparseable -> None.
    assert_eq!(parse_memory_string("shared"), None);
    assert_eq!(parse_memory_string(""), None);
    assert_eq!(parse_memory_string("8"), None);
}

#[test]
fn gpu_adapter_json_extracts_brand_and_vram() {
    let row: serde_json::Value = serde_json::json!({
        "_name": "AMD Radeon Pro 5500 XT",
        "spdisplays_vendor-id": "0x1002",
        "spdisplays_device-id": "0x7340",
        "spdisplays_vram": "8 GB"
    });
    let adapter = gpu_adapter_from_json(&row).expect("discrete GPU row maps");
    assert!(adapter.identity_is_authoritative);
    assert!(adapter.identity.contains("0x1002"));
    assert!(adapter.identity.contains("0x7340"));
    assert_eq!(adapter.brand, "AMD Radeon Pro 5500 XT");
    assert_eq!(adapter.vram_total_bytes, Some(8 * 1024 * 1024 * 1024));
}

#[test]
fn inventory_only_gpu_marks_live_throttle_capability_unavailable() {
    let adapter = MacGpuAdapter {
        identity: "spdisplays_device-id=0x7340".into(),
        identity_is_authoritative: true,
        brand: "AMD Radeon Pro 5500 XT".into(),
        vram_total_bytes: Some(8 * 1024 * 1024 * 1024),
    };
    let observation = gpu_observation_from_profiler(Ok(vec![adapter]), 42);
    let gpu = &observation.current_value().expect("current inventory")[0];

    assert_eq!(
        gpu.current_dedicated_vram_total_bytes(),
        Some(8 * 1024 * 1024 * 1024)
    );
    assert_eq!(gpu.current_throttle_reasons(), None);
    assert_eq!(
        gpu.throttle_observation().availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
}

#[test]
fn gpu_adapter_json_apple_silicon_has_no_vram() {
    // Unified-memory Apple Silicon parts expose a brand but no vram field;
    // the absence is honest (None), never a fabricated capacity.
    let row: serde_json::Value =
        serde_json::json!({ "_name": "Apple M1 Pro", "spdisplays_gpu_core_count": "16" });
    let adapter = gpu_adapter_from_json(&row).expect("apple silicon GPU row maps");
    assert!(!adapter.identity_is_authoritative);
    assert_eq!(adapter.brand, "Apple M1 Pro");
    assert_eq!(adapter.vram_total_bytes, None);
}

#[test]
fn gpu_adapter_json_rejects_display_only_rows() {
    // Rows without a GPU identity must not produce a fabricated adapter.
    let row: serde_json::Value = serde_json::json!({ "spdisplays_display_type": "LCD" });
    assert!(gpu_adapter_from_json(&row).is_none());
}

#[test]
fn duplicate_gpu_identity_is_not_disambiguated_by_enumeration_index() {
    let adapter = || MacGpuAdapter {
        identity: "spdisplays_device-id=0x1234".to_string(),
        identity_is_authoritative: true,
        brand: "GPU".to_string(),
        vram_total_bytes: None,
    };
    let observation = gpu_observation_from_profiler(Ok(vec![adapter(), adapter()]), 1);
    assert_eq!(observation.current_value().expect("current GPUs").len(), 1);
    assert_eq!(
        observation.sources()[0].outcome,
        SourceOutcome::Partial(FailureKind::Unsupported)
    );
}
