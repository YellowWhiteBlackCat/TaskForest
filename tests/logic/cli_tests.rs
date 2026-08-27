//! Tests for the toolkit-neutral CLI module: argv dispatch, the snapshot
//! envelope's honesty rules, and the threshold-suggestion JSON rendering.

use super::suggest::{shape_suggestion, suggest_thresholds_json};
use super::*;
use taskmanager_core::core::alerts::{
    AlertMetric, SUGGESTION_MIN_SAMPLES, SuggestedThreshold, SuggestionBasis, SuggestionConfidence,
};
use taskmanager_core::core::export::snapshot_to_json;
use taskmanager_core::{CpuMetrics, CpuScalarObservations, ScalarObservation};

#[test]
fn parse_args_dispatches_flag_modes() {
    assert_eq!(
        parse_args([] as [String; 0]),
        Ok(CliMode::Gui {
            app_id: None,
            demo: false
        })
    );
    assert_eq!(
        parse_args(["--app-id".into(), "org.example.TaskManager".into()]),
        Ok(CliMode::Gui {
            app_id: Some("org.example.TaskManager".into()),
            demo: false
        })
    );
    assert_eq!(
        parse_args(["-a".into(), "org.example.TaskManager".into()]),
        Ok(CliMode::Gui {
            app_id: Some("org.example.TaskManager".into()),
            demo: false
        })
    );
    assert_eq!(
        parse_args(["--app-id=org.example.TaskManager".into()]),
        Ok(CliMode::Gui {
            app_id: Some("org.example.TaskManager".into()),
            demo: false
        })
    );
    assert_eq!(
        parse_args(["--demo".into()]),
        Ok(CliMode::Gui {
            app_id: None,
            demo: true
        })
    );
    assert_eq!(parse_args(["--json".into()]), Ok(CliMode::JsonSnapshot));
    assert_eq!(parse_args(["-j".into()]), Ok(CliMode::JsonSnapshot));
    assert_eq!(
        parse_args(["--capture-window".into(), "target/win-evidence".into()]),
        Ok(CliMode::CaptureWindow {
            out: std::path::PathBuf::from("target/win-evidence")
        })
    );
    assert_eq!(
        parse_args(["--capture-window".into()]),
        Err(CliArgError::MissingCaptureOutput)
    );
    assert_eq!(
        parse_args(["--capture-window".into(), "target".into(), "extra".into()]),
        Err(CliArgError::UnknownArgument)
    );
    assert_eq!(
        parse_args(["--suggest-thresholds".into()]),
        Ok(CliMode::SuggestThresholds)
    );
    assert_eq!(
        parse_args(["--gpu-engines".into()]),
        Ok(CliMode::GpuEngines)
    );
    assert_eq!(parse_args(["--help".into()]), Ok(CliMode::Help));
    assert_eq!(parse_args(["-h".into()]), Ok(CliMode::Help));
}

#[test]
fn parse_args_rejects_unknown_and_trailing_tokens() {
    assert_eq!(
        parse_args(["--bogus".into()]),
        Err(CliArgError::UnknownArgument)
    );
    // A trailing argument after --json would silently change the output a
    // script receives, so it is rejected rather than ignored.
    assert_eq!(
        parse_args(["--json".into(), "extra".into()]),
        Err(CliArgError::UnknownArgument)
    );
    assert_eq!(
        parse_args(["--help".into(), "extra".into()]),
        Err(CliArgError::UnknownArgument)
    );
    assert_eq!(
        parse_args(["--suggest-thresholds".into(), "extra".into()]),
        Err(CliArgError::UnknownArgument)
    );
    assert_eq!(
        parse_args(["--gpu-engines".into(), "extra".into()]),
        Err(CliArgError::UnknownArgument)
    );
    assert_eq!(
        parse_args(["--app-id".into()]),
        Err(CliArgError::MissingApplicationId)
    );
    assert_eq!(
        parse_args(["--app-id".into(), "not-an-id".into()]),
        Err(CliArgError::InvalidApplicationId)
    );
    assert_eq!(
        parse_args(["--app-id=".into()]),
        Err(CliArgError::InvalidApplicationId)
    );
    assert_eq!(
        parse_args(["--app-id".into(), "org.example.App".into(), "extra".into()]),
        Err(CliArgError::UnknownArgument)
    );
}

#[test]
fn unavailable_fields_serialize_as_null_not_zero() {
    // A snapshot with an explicitly unavailable CPU temperature and brand,
    // and no GPU devices at all. None of these may become a fabricated 0.
    let snapshot = SystemSnapshot {
        timestamp_ms: 1,
        cpu: CpuMetrics::default(),
        gpu: Vec::new(),
        ..SystemSnapshot::default()
    };
    let json = snapshot_to_json(&snapshot, &[]);
    let value: serde_json::Value = serde_json::from_str(&json).expect("snapshot JSON must parse");

    let cpu = &value["snapshot"]["cpu"];
    // Stable export fields preserve unavailability as JSON null, never as a
    // fabricated numeric value. Optional static identity remains omitted.
    assert_eq!(cpu.get("temperature_c"), Some(&serde_json::Value::Null));
    assert!(cpu.get("brand").is_none());
    // No GPU serializes as an empty array, never a fabricated zero-valued card.
    assert_eq!(
        value["snapshot"]["gpu"],
        serde_json::Value::Array(Vec::new())
    );
    // Top-level domains are always present in the envelope.
    for key in [
        "timestamp_ms",
        "cpu",
        "memory",
        "disks",
        "networks",
        "gpu",
        "uptime_secs",
        "processes",
        "threads",
    ] {
        assert!(
            value["snapshot"].get(key).is_some(),
            "snapshot envelope must include domain {key}"
        );
    }
    assert_eq!(value["processes"], serde_json::Value::Array(Vec::new()));
    // The three additive extras keys are always present as honest empty
    // arrays — never absent and never carrying a fabricated sentinel row.
    for key in ["containers", "process_gpu_engines", "suggested_thresholds"] {
        assert!(
            value[key].is_array(),
            "envelope must always carry the additive key {key} as an array"
        );
        assert_eq!(
            value[key].as_array().map(Vec::len),
            Some(0),
            "{key} must be empty when no extras are supplied"
        );
    }
}

#[test]
fn snapshot_cli_error_surfaces_a_stable_stage_code_and_detail() {
    let error = SnapshotCliError::new(
        SnapshotCliErrorKind::CollectionTimeout,
        "no complete snapshot within 5000 ms",
    );
    assert_eq!(error.kind(), SnapshotCliErrorKind::CollectionTimeout);
    assert_eq!(error.kind().code(), "collection_timeout");
    assert_eq!(error.detail(), "no complete snapshot within 5000 ms");
    let rendered = error.to_string();
    assert!(rendered.starts_with("collection_timeout: "), "{rendered}");
    assert!(rendered.contains("5000 ms"), "{rendered}");
}

#[test]
fn suggest_thresholds_json_default_snapshot_is_honestly_null_or_unsupported() {
    // A bare default snapshot carries no finite CPU/memory sample and no
    // disks. Every numeric metric must serialize as JSON null (honest
    // absence); the binary SMART-warning metric must name itself as an
    // unsupported metric. Nothing may be a fabricated threshold.
    let value: serde_json::Value =
        serde_json::from_str(&suggest_thresholds_json(&SystemSnapshot::default()))
            .expect("threshold JSON must parse");

    for key in [
        "cpu_usage_percent",
        "memory_usage_percent",
        "disk_temperature_c",
        "smart_percent_used",
    ] {
        assert!(
            value[key].is_null(),
            "{key} with no observed sample must be JSON null, not a fabricated threshold"
        );
    }
    let binary = &value["smart_critical_warning"];
    assert_eq!(binary["status"], "insufficient");
    assert_eq!(binary["reason"], "unsupported_metric");
    assert!(
        binary.get("threshold").is_none(),
        "an unsupported metric must never carry a fabricated threshold"
    );
}

#[test]
fn suggest_thresholds_json_with_one_sample_reports_too_few_samples() {
    // One observed CPU sample is honest data, but a single point is below
    // the principled 20-sample floor: the engine's TooFewSamples verdict
    // must be surfaced with its sample_count, and never a threshold value.
    let snapshot = SystemSnapshot {
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(50.0, 1),
            ..Default::default()
        }),
        ..SystemSnapshot::default()
    };
    let value: serde_json::Value = serde_json::from_str(&suggest_thresholds_json(&snapshot))
        .expect("threshold JSON must parse");
    let cpu = &value["cpu_usage_percent"];
    assert_eq!(cpu["status"], "insufficient");
    assert_eq!(cpu["reason"], "too_few_samples");
    assert_eq!(cpu["sample_count"], 1);
    assert_eq!(cpu["required"], SUGGESTION_MIN_SAMPLES);
    assert!(
        cpu.get("threshold").is_none(),
        "a TooFewSamples verdict must never carry a fabricated threshold"
    );
    // Memory had no observed sample (no total_bytes) → still null.
    assert!(value["memory_usage_percent"].is_null());
}

#[test]
fn suggest_thresholds_json_aggregates_per_disk_samples_but_still_insufficient() {
    // Two disks each contribute one temperature sample (sample_count = 2),
    // still far below the 20-sample floor. The verdict must reflect the
    // aggregated count honestly, not invent a threshold.
    let snapshot = SystemSnapshot {
        disks: vec![
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .smart_temperature_c(Some(40.0))
                .build(),
            taskmanager_test_support::DiskMetricsFixtureBuilder::new()
                .smart_temperature_c(Some(45.0))
                .build(),
        ],
        ..SystemSnapshot::default()
    };
    let value: serde_json::Value = serde_json::from_str(&suggest_thresholds_json(&snapshot))
        .expect("threshold JSON must parse");
    let temp = &value["disk_temperature_c"];
    assert_eq!(temp["status"], "insufficient");
    assert_eq!(temp["reason"], "too_few_samples");
    assert_eq!(temp["sample_count"], 2);
    assert!(
        temp.get("threshold").is_none(),
        "insufficient disk-temperature verdict must not fabricate a threshold"
    );
}

#[test]
fn shape_suggestion_renders_a_suggested_verdict_with_its_derivation() {
    // Directly exercise the Suggested branch (unreachable from one snapshot
    // via the CLI, but the shaper must still render it correctly when a
    // caller feeds a principled window from history).
    let suggested = SuggestedThreshold::Suggested {
        metric: AlertMetric::MemoryUsagePercent,
        threshold: 72.0,
        // 3.5 (not 3.6) so the f32→f64 round-trip is bit-exact and the
        // assertion compares numbers, not float noise.
        hysteresis: 3.5,
        basis: SuggestionBasis::MeanPlusStddevFloorP95,
        sample_count: 64,
        confidence: SuggestionConfidence::High,
    };
    let shaped = shape_suggestion(&suggested);
    assert_eq!(shaped["status"], "suggested");
    assert_eq!(shaped["metric"], "memory_usage_percent");
    assert_eq!(shaped["threshold"], 72.0);
    assert_eq!(shaped["hysteresis"], 3.5);
    assert_eq!(shaped["basis"], "mean_plus_stddev_floor_p95");
    assert_eq!(shaped["sample_count"], 64);
    assert_eq!(shaped["confidence"], "high");
}
