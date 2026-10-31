use super::*;
use taskforest_core::{DeviceGeneration, DevicePresence, ScalarAvailability};

fn trusted_adapter(identity: &str, brand: &str) -> MacGpuAdapter {
    MacGpuAdapter {
        identity: Some(identity.to_string()),
        brand: brand.to_string(),
        vram_total_bytes: None,
    }
}

fn enumeration(adapters: Vec<MacGpuAdapter>) -> MacGpuEnumeration {
    MacGpuEnumeration {
        raw_item_count: adapters.len(),
        adapters,
        failure: None,
    }
}

fn refresh(
    lifecycles: &mut DeviceLifecycleRegistry,
    inventory: Result<MacGpuEnumeration, FailureKind>,
    dynamic: Result<Option<f32>, FailureKind>,
    observed_at_ms: u64,
) -> GpuTelemetryObservation {
    gpu_observation_from_profiler_with_dynamic(lifecycles, inventory, dynamic, observed_at_ms)
}

fn lifecycle<'a>(observation: &'a GpuTelemetryObservation, id: &str) -> &'a DeviceLifecycle {
    observation
        .device_lifecycles()
        .get(&DeviceId::new(id))
        .expect("lifecycle sidecar contains the tracked identity")
}

#[test]
fn profiler_failure_is_unavailable_and_does_not_confirm_absence() {
    let mut lifecycles = DeviceLifecycleRegistry::new(30_000);
    let first = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter("pci-path-0", "AMD GPU")])),
        Ok(None),
        1,
    );
    let id = "macos:gpu:system-profiler:pci-path-0";
    assert_eq!(
        first.current_value().expect("first inventory")[0].device_generation,
        DeviceGeneration::INITIAL
    );

    let unavailable = refresh(
        &mut lifecycles,
        Err(FailureKind::MissingDependency),
        Ok(None),
        2,
    );
    assert!(unavailable.current_value().is_none());
    assert!(unavailable.sources().iter().any(|source| {
        source.outcome == SourceOutcome::Unavailable(FailureKind::MissingDependency)
    }));
    assert_eq!(
        lifecycle(&unavailable, id).presence,
        DevicePresence::Unavailable
    );
    assert_eq!(
        lifecycle(&unavailable, id).generation,
        DeviceGeneration::INITIAL
    );

    let recovered = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter("pci-path-0", "AMD GPU")])),
        Ok(None),
        3,
    );
    assert_eq!(
        recovered.current_value().expect("recovered inventory")[0].device_generation,
        DeviceGeneration::INITIAL
    );
}

#[test]
fn repeated_trusted_unique_identity_keeps_its_generation_and_sidecar() {
    let mut lifecycles = DeviceLifecycleRegistry::new(30_000);
    let first = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter("pci-path-0", "AMD GPU")])),
        Ok(None),
        10,
    );
    let second = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter("pci-path-0", "AMD GPU")])),
        Ok(None),
        20,
    );
    let id = "macos:gpu:system-profiler:pci-path-0";

    let first_gpu = &first.current_value().expect("first GPU snapshot")[0];
    let second_gpu = &second.current_value().expect("second GPU snapshot")[0];
    assert_eq!(first_gpu.device_id, id);
    assert_eq!(second_gpu.device_generation, DeviceGeneration::INITIAL);
    assert_eq!(lifecycle(&second, id).presence, DevicePresence::Present);
    assert_eq!(lifecycle(&second, id).generation, DeviceGeneration::INITIAL);
}

#[test]
fn complete_empty_inventory_confirms_absence_and_reappearance_advances_generation() {
    let mut lifecycles = DeviceLifecycleRegistry::new(30_000);
    let id = "macos:gpu:system-profiler:pci-path-0";
    let first = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter("pci-path-0", "AMD GPU")])),
        Ok(None),
        10,
    );
    assert_eq!(
        first.current_value().expect("initial inventory")[0].device_generation,
        DeviceGeneration::INITIAL
    );

    let empty_enumeration = parse_system_profiler_gpus(br#"{"SPDisplaysDataType":[]}"#)
        .expect("valid empty profiler result");
    let empty = refresh(&mut lifecycles, Ok(empty_enumeration), Ok(None), 20);
    assert!(
        empty
            .current_value()
            .expect("complete empty inventory")
            .is_empty()
    );
    assert_eq!(empty.sources()[0].outcome, SourceOutcome::Empty);
    assert_eq!(lifecycle(&empty, id).presence, DevicePresence::Absent);

    let returned = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter("pci-path-0", "AMD GPU")])),
        Ok(None),
        30,
    );
    assert_eq!(
        returned.current_value().expect("returned inventory")[0].device_generation,
        DeviceGeneration::new(2)
    );
    assert_eq!(
        lifecycle(&returned, id).generation,
        DeviceGeneration::new(2)
    );
}

#[test]
fn partial_inventory_does_not_mark_omitted_trusted_device_absent() {
    let mut lifecycles = DeviceLifecycleRegistry::new(30_000);
    let id = "macos:gpu:system-profiler:pci-path-existing";
    let _initial = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter(
            "pci-path-existing",
            "Intel GPU",
        )])),
        Ok(None),
        1,
    );
    let partial_inventory = MacGpuEnumeration {
        adapters: vec![trusted_adapter("pci-path-new", "AMD GPU")],
        failure: Some(FailureKind::ProviderFault),
        raw_item_count: 2,
    };

    let partial = refresh(&mut lifecycles, Ok(partial_inventory), Ok(None), 2);

    assert_eq!(
        partial.sources()[0].outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(
        lifecycle(&partial, id).presence,
        DevicePresence::Unavailable
    );
    assert_eq!(
        lifecycle(&partial, id).generation,
        DeviceGeneration::INITIAL
    );
}

#[test]
fn duplicate_identity_rows_are_visible_without_generation_and_do_not_confirm_absence() {
    let mut lifecycles = DeviceLifecycleRegistry::new(30_000);
    let id = "macos:gpu:system-profiler:pci-path-0";
    let _initial = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter("pci-path-0", "AMD GPU")])),
        Ok(None),
        1,
    );

    let duplicate = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![
            trusted_adapter("pci-path-0", "AMD GPU A"),
            trusted_adapter("pci-path-0", "AMD GPU B"),
        ])),
        Ok(None),
        2,
    );

    let rows = duplicate
        .current_value()
        .expect("partial inventory remains visible");
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|gpu| gpu.device_id.is_empty() && !gpu.device_generation.is_valid())
    );
    assert_eq!(
        duplicate.sources()[0].outcome,
        SourceOutcome::Partial(FailureKind::Unsupported)
    );
    assert_eq!(
        lifecycle(&duplicate, id).presence,
        DevicePresence::Unavailable
    );
    assert_eq!(
        lifecycle(&duplicate, id).generation,
        DeviceGeneration::INITIAL
    );
}

#[test]
fn malformed_nonempty_profiler_input_is_partial_not_empty() {
    let parsed = parse_system_profiler_gpus(
        br#"{"SPDisplaysDataType":[{"_name":"Visible GPU"},{"malformed":true}]}"#,
    )
    .expect("well-formed JSON can retain its parseable row");
    assert_eq!(parsed.raw_item_count, 2);
    assert_eq!(parsed.adapters.len(), 1);
    assert_eq!(parsed.failure, Some(FailureKind::ProviderFault));

    let mut lifecycles = DeviceLifecycleRegistry::new(30_000);
    let old_id = "macos:gpu:system-profiler:pci-path-old";
    let _initial = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter(
            "pci-path-old",
            "Intel GPU",
        )])),
        Ok(None),
        0,
    );
    let malformed = refresh(&mut lifecycles, Ok(parsed), Ok(None), 1);
    assert_eq!(
        malformed.sources()[0].outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert!(!matches!(
        malformed.sources()[0].outcome,
        SourceOutcome::Empty
    ));
    assert_eq!(
        malformed
            .current_value()
            .expect("known row stays visible")
            .len(),
        1
    );
    assert!(
        malformed.current_value().expect("known row stays visible")[0]
            .device_id
            .is_empty()
    );
    assert_eq!(
        lifecycle(&malformed, old_id).presence,
        DevicePresence::Unavailable
    );

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
fn wholly_unrecognized_nonempty_inventory_does_not_confirm_absence() {
    let parsed = parse_system_profiler_gpus(
        br#"{"SPDisplaysDataType":[{"spdisplays_display_type":"LCD"}]}"#,
    )
    .expect("valid JSON array with an unrecognized row");
    assert_eq!(parsed.raw_item_count, 1);
    assert!(parsed.adapters.is_empty());
    assert_eq!(parsed.failure, Some(FailureKind::ProviderFault));

    let mut lifecycles = DeviceLifecycleRegistry::new(30_000);
    let old_id = "macos:gpu:system-profiler:pci-path-old";
    let _initial = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter(
            "pci-path-old",
            "Intel GPU",
        )])),
        Ok(None),
        0,
    );
    let malformed = refresh(&mut lifecycles, Ok(parsed), Ok(None), 1);

    assert_eq!(
        malformed.sources()[0].outcome,
        SourceOutcome::Partial(FailureKind::ProviderFault)
    );
    assert!(
        malformed
            .current_value()
            .expect("partial inventory is represented")
            .is_empty()
    );
    assert_eq!(
        lifecycle(&malformed, old_id).presence,
        DevicePresence::Unavailable
    );
}

#[test]
fn profiler_model_fields_and_brand_do_not_become_stable_identity() {
    let row: serde_json::Value = serde_json::json!({
        "_name": "AMD Radeon Pro 5500 XT",
        "spdisplays_vendor-id": "0x1002",
        "spdisplays_device-id": "0x7340",
        "spdisplays_revision-id": "0x0040",
        "spdisplays_gmux-version": "5.0.0",
        "spdisplays_vram": "8 GB"
    });
    let adapter = gpu_adapter_from_json(&row).expect("GPU model row maps");
    assert_eq!(adapter.identity, None);
    assert_eq!(adapter.brand, "AMD Radeon Pro 5500 XT");
    assert_eq!(adapter.vram_total_bytes, Some(8 * 1024 * 1024 * 1024));

    let mut lifecycles = DeviceLifecycleRegistry::new(30_000);
    let old_id = "macos:gpu:system-profiler:pci-path-old";
    let _initial = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter(
            "pci-path-old",
            "Intel GPU",
        )])),
        Ok(None),
        40,
    );
    let observation = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![adapter])),
        Ok(None),
        42,
    );
    let gpu = &observation
        .current_value()
        .expect("untrusted row remains visible")[0];
    assert!(gpu.device_id.is_empty());
    assert!(!gpu.device_generation.is_valid());
    assert_eq!(
        gpu.current_dedicated_vram_total_bytes(),
        Some(8 * 1024 * 1024 * 1024)
    );
    assert_eq!(
        observation.sources()[0].outcome,
        SourceOutcome::Partial(FailureKind::Unsupported)
    );
    assert_eq!(
        lifecycle(&observation, old_id).presence,
        DevicePresence::Unavailable
    );
}

#[test]
fn apple_silicon_row_keeps_brand_without_vram_or_brand_fallback_identity() {
    let row: serde_json::Value =
        serde_json::json!({ "_name": "Apple M1 Pro", "spdisplays_gpu_core_count": "16" });
    let adapter = gpu_adapter_from_json(&row).expect("apple silicon GPU row maps");
    assert_eq!(adapter.identity, None);
    assert_eq!(adapter.brand, "Apple M1 Pro");
    assert_eq!(adapter.vram_total_bytes, None);
}

#[test]
fn ioreg_enrichment_failure_does_not_change_complete_empty_discovery() {
    let mut lifecycles = DeviceLifecycleRegistry::new(30_000);
    let id = "macos:gpu:system-profiler:pci-path-0";
    let _initial = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter("pci-path-0", "AMD GPU")])),
        Ok(None),
        1,
    );
    let empty_enumeration = parse_system_profiler_gpus(br#"{"SPDisplaysDataType":[]}"#)
        .expect("valid empty profiler result");

    let empty = refresh(
        &mut lifecycles,
        Ok(empty_enumeration),
        Err(FailureKind::TimedOut),
        2,
    );

    assert_eq!(empty.sources()[0].outcome, SourceOutcome::Empty);
    assert!(empty.sources().iter().any(|source| {
        source.provider == GPU_IOREG_PROVIDER
            && source.outcome == SourceOutcome::Unavailable(FailureKind::TimedOut)
    }));
    assert_eq!(lifecycle(&empty, id).presence, DevicePresence::Absent);
}

#[test]
fn memory_string_parses_iec_units_to_bytes() {
    assert_eq!(parse_memory_string("8 GB"), Some(8 * 1024 * 1024 * 1024));
    assert_eq!(parse_memory_string("16384 MB"), Some(16_384 * 1024 * 1024));
    assert_eq!(parse_memory_string("2048 KB"), Some(2048 * 1024));
    assert_eq!(parse_memory_string("0 B"), Some(0));
    assert_eq!(parse_memory_string("shared"), None);
    assert_eq!(parse_memory_string(""), None);
    assert_eq!(parse_memory_string("8"), None);
}

#[test]
fn inventory_only_gpu_marks_live_throttle_capability_unavailable() {
    let mut lifecycles = DeviceLifecycleRegistry::new(30_000);
    let observation = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![MacGpuAdapter {
            identity: None,
            brand: "AMD Radeon Pro 5500 XT".into(),
            vram_total_bytes: Some(8 * 1024 * 1024 * 1024),
        }])),
        Ok(None),
        42,
    );
    let gpu = &observation.current_value().expect("current inventory")[0];

    assert_eq!(
        gpu.current_dedicated_vram_total_bytes(),
        Some(8 * 1024 * 1024 * 1024)
    );
    assert_eq!(gpu.current_throttle_reasons(), None);
    assert_eq!(
        gpu.throttle_observation().availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert!(!gpu.device_generation.is_valid());
}

#[test]
fn ioreg_gpu_utilization_parser_keeps_only_bounded_device_values() {
    let output = r#"
        "PerformanceStatistics" = {"Device Utilization %"=37,"GPU Activity(%)"=41}
        "PerformanceStatistics" = {"Device Utilization %"=101}
    "#;
    assert_eq!(parse_ioreg_gpu_utilizations(output), vec![37.0]);
    assert_eq!(
        parse_ioreg_gpu_utilizations("\"GPU Activity(%)\"=41"),
        vec![41.0]
    );
}

#[test]
fn one_ioreg_value_is_promoted_when_one_adapter_row_can_be_matched() {
    let mut lifecycles = DeviceLifecycleRegistry::new(30_000);
    let observation = refresh(
        &mut lifecycles,
        Ok(enumeration(vec![trusted_adapter("pci-path-0", "AMD GPU")])),
        Ok(Some(37.0)),
        42,
    );
    assert_eq!(
        observation.current_value().expect("current GPU")[0].current_utilization_pct(),
        Some(37.0)
    );
}

#[test]
fn display_only_rows_are_not_mapped_to_gpu_adapter_facts() {
    let row: serde_json::Value = serde_json::json!({ "spdisplays_display_type": "LCD" });
    assert!(gpu_adapter_from_json(&row).is_none());

    let inventory = parse_system_profiler_gpus(
        br#"{"SPDisplaysDataType":[{"spdisplays_display_type":"LCD"}]}"#,
    )
    .expect("array can be parsed");
    assert_eq!(inventory.adapters.len(), 0);
    assert_eq!(inventory.failure, Some(FailureKind::ProviderFault));
}
