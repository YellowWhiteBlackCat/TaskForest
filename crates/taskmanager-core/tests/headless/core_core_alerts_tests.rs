use super::*;
use crate::core::FailureKind;
use crate::core::metrics::{
    CpuMetrics, CpuScalarObservations, DiskMetrics, MemoryMetrics, MemoryScalarObservations,
    ScalarObservation,
};

fn disk_metrics(
    device_id: &str,
    name: &str,
    temperature_c: Option<f32>,
    percent_used: Option<f32>,
    critical_warning: Option<bool>,
) -> DiskMetrics {
    let mut disk = DiskMetrics::new(name);
    disk.device_id = device_id.to_owned();
    disk.smart_temperature_c = temperature_c;
    disk.smart_percent_used = percent_used;
    disk.smart_critical_warning = critical_warning;
    disk
}

fn snapshot(at_ms: u64, cpu: f32, memory: f32) -> SystemSnapshot {
    SystemSnapshot {
        timestamp_ms: at_ms,
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(cpu, at_ms),
            ..Default::default()
        }),
        memory: MemoryMetrics::from_observations(
            MemoryScalarObservations {
                total_bytes: ScalarObservation::available(100, at_ms),
                used_bytes: ScalarObservation::available(memory as u64, at_ms),
                available_bytes: ScalarObservation::available(100 - memory as u64, at_ms),
                ..Default::default()
            },
            Default::default(),
        ),
        ..Default::default()
    }
}

fn rule(metric: AlertMetric, duration_ms: u64, hysteresis: f32) -> AlertRule {
    AlertRule::new(
        "resource-hot",
        metric,
        AlertSeverity::Warning,
        80.0,
        Duration::from_millis(duration_ms),
        hysteresis,
    )
}

#[test]
fn cpu_requires_duration_and_repeated_evaluation_is_deduplicated() {
    let mut engine = AlertEngine::new([rule(AlertMetric::CpuUsagePercent, 5_000, 5.0)]);
    assert!(engine.evaluate(&snapshot(1_000, 90.0, 0.0)).is_empty());
    assert!(engine.evaluate(&snapshot(5_999, 95.0, 0.0)).is_empty());
    let first = engine.evaluate(&snapshot(6_000, 91.0, 0.0));
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].active_since_ms, 6_000);

    let repeated = engine.evaluate(&snapshot(7_000, 93.0, 0.0));
    assert_eq!(repeated.len(), 1);
    assert_eq!(repeated[0].instance_id, "resource-hot:system");
    assert_eq!(repeated[0].active_since_ms, 6_000);
}

#[test]
fn hysteresis_prevents_flapping_then_clears_below_clear_threshold() {
    let mut engine = AlertEngine::new([rule(AlertMetric::CpuUsagePercent, 0, 10.0)]);
    assert_eq!(engine.evaluate(&snapshot(0, 85.0, 0.0)).len(), 1);
    assert_eq!(engine.evaluate(&snapshot(1, 75.0, 0.0)).len(), 1);
    assert!(engine.evaluate(&snapshot(2, 70.0, 0.0)).is_empty());
    assert_eq!(engine.evaluate(&snapshot(3, 90.0, 0.0)).len(), 1);
}

#[test]
fn unavailable_cpu_usage_clears_pending_and_never_uses_legacy_projection() {
    let mut engine = AlertEngine::new([rule(AlertMetric::CpuUsagePercent, 1_000, 5.0)]);
    assert!(engine.evaluate(&snapshot(0, 90.0, 0.0)).is_empty());
    let unavailable = SystemSnapshot {
        timestamp_ms: 1_000,
        cpu: CpuMetrics::from_observations(CpuScalarObservations::unavailable(
            FailureKind::PermissionDenied,
        )),
        ..Default::default()
    };

    assert!(engine.evaluate(&unavailable).is_empty());
    assert!(
        engine.evaluate(&snapshot(1_001, 95.0, 0.0)).is_empty(),
        "recovery starts a fresh duration instead of inheriting pending time"
    );
}

#[test]
fn memory_disk_temperature_and_smart_thresholds_are_platform_neutral() {
    let mut snap = snapshot(100, 0.0, 85.0);
    snap.disks = vec![disk_metrics(
        "wwid-1",
        "nvme0n1",
        Some(76.0),
        Some(92.0),
        Some(true),
    )];
    let rules = [
        rule(AlertMetric::MemoryUsagePercent, 0, 5.0),
        AlertRule::new(
            "disk-hot",
            AlertMetric::DiskTemperatureC,
            AlertSeverity::Critical,
            70.0,
            Duration::ZERO,
            5.0,
        ),
        AlertRule::new(
            "smart-wear",
            AlertMetric::SmartPercentUsed,
            AlertSeverity::Warning,
            90.0,
            Duration::ZERO,
            5.0,
        ),
        AlertRule::new(
            "smart-warning",
            AlertMetric::SmartCriticalWarning,
            AlertSeverity::Critical,
            1.0,
            Duration::ZERO,
            1.0,
        ),
    ];
    let alerts = AlertEngine::new(rules).evaluate(&snap);
    assert_eq!(alerts.len(), 4);
    assert!(alerts.iter().any(|a| a.rule_id == "resource-hot"));
    assert!(alerts.iter().any(|a| a.rule_id == "disk-hot"));
    assert!(alerts.iter().any(|a| a.rule_id == "smart-wear"));
    assert!(alerts.iter().any(|a| a.rule_id == "smart-warning"));
}

#[test]
fn missing_disk_sensor_drops_active_state_and_target_filter_is_honored() {
    let targeted = AlertRule::new(
        "disk-hot",
        AlertMetric::DiskTemperatureC,
        AlertSeverity::Warning,
        70.0,
        Duration::from_secs(1),
        5.0,
    )
    .for_target("wanted");
    let mut engine = AlertEngine::new([targeted]);
    let mut snap = snapshot(0, 0.0, 0.0);
    snap.disks = vec![
        disk_metrics("", "ignored", Some(99.0), None, None),
        disk_metrics("", "wanted", Some(80.0), None, None),
    ];
    assert!(engine.evaluate(&snap).is_empty());
    snap.timestamp_ms = 1_000;
    assert_eq!(engine.evaluate(&snap).len(), 1);
    snap.disks[1].smart_temperature_c = None;
    snap.timestamp_ms = 2_000;
    assert!(engine.evaluate(&snap).is_empty());
    snap.disks[1].smart_temperature_c = Some(80.0);
    snap.timestamp_ms = 3_000;
    assert!(engine.evaluate(&snap).is_empty());
}

#[test]
fn invalid_and_duplicate_rules_do_not_create_duplicate_alerts() {
    let valid = rule(AlertMetric::CpuUsagePercent, 0, 0.0);
    let duplicate = valid.clone();
    let invalid = AlertRule::new(
        "",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Info,
        1.0,
        Duration::ZERO,
        0.0,
    );
    let mut engine = AlertEngine::new([valid, duplicate, invalid]);
    assert_eq!(engine.rules().len(), 1);
    assert_eq!(engine.evaluate(&snapshot(0, 90.0, 0.0)).len(), 1);
}

fn transfer_entry(id: &str, threshold: f32, enabled: bool) -> AlertRuleTransferEntry {
    AlertRuleTransferEntry::new(
        AlertRule::new(
            id,
            AlertMetric::CpuUsagePercent,
            AlertSeverity::Warning,
            threshold,
            Duration::from_millis(5_000),
            5.0,
        ),
        enabled,
    )
}

#[test]
fn alert_rule_json_v1_is_stable_and_round_trips_enabled_state() {
    let entry = transfer_entry("cpu-high", 90.0, false);
    let json = export_alert_rules_json(std::slice::from_ref(&entry)).unwrap();
    assert_eq!(
        json,
        concat!(
            "{\n",
            "  \"schema\": \"taskforest.alert-rules\",\n",
            "  \"version\": 1,\n",
            "  \"rules\": [\n",
            "    {\n",
            "      \"id\": \"cpu-high\",\n",
            "      \"metric\": \"cpu_usage_percent\",\n",
            "      \"severity\": \"warning\",\n",
            "      \"threshold\": 90.0,\n",
            "      \"for_duration_ms\": 5000,\n",
            "      \"hysteresis\": 5.0,\n",
            "      \"target\": null,\n",
            "      \"enabled\": false\n",
            "    }\n",
            "  ]\n",
            "}\n",
        )
    );
    assert_eq!(import_alert_rules_json(&json).unwrap(), vec![entry]);
}

#[test]
fn alert_rule_import_strictly_rejects_unknown_version_fields_and_bad_rules() {
    let unsupported = r#"{
            "schema":"taskforest.alert-rules",
            "version":2,
            "rules":[]
        }"#;
    assert_eq!(
        import_alert_rules_json(unsupported),
        Err(AlertRuleTransferError::UnsupportedVersion(2))
    );

    let unknown_field = r#"{
            "schema":"taskforest.alert-rules",
            "version":1,
            "rules":[],
            "replace":true
        }"#;
    assert!(matches!(
        import_alert_rules_json(unknown_field),
        Err(AlertRuleTransferError::InvalidJson(_))
    ));

    let duplicate = r#"{
            "schema":"taskforest.alert-rules",
            "version":1,
            "rules":[
                {"id":"cpu","metric":"cpu_usage_percent","severity":"info","threshold":80.0,"for_duration_ms":0,"hysteresis":5.0,"target":null,"enabled":true},
                {"id":"cpu","metric":"cpu_usage_percent","severity":"critical","threshold":90.0,"for_duration_ms":0,"hysteresis":5.0,"target":null,"enabled":false}
            ]
        }"#;
    assert_eq!(
        import_alert_rules_json(duplicate),
        Err(AlertRuleTransferError::DuplicateRuleId("cpu".into()))
    );

    let out_of_range = r#"{
            "schema":"taskforest.alert-rules",
            "version":1,
            "rules":[
                {"id":"cpu","metric":"cpu_usage_percent","severity":"info","threshold":101.0,"for_duration_ms":0,"hysteresis":5.0,"target":null,"enabled":true}
            ]
        }"#;
    assert!(matches!(
        import_alert_rules_json(out_of_range),
        Err(AlertRuleTransferError::InvalidRule {
            index: 0,
            field: "threshold",
            ..
        })
    ));

    let target_on_system_rule = r#"{
            "schema":"taskforest.alert-rules",
            "version":1,
            "rules":[
                {"id":"cpu","metric":"cpu_usage_percent","severity":"info","threshold":80.0,"for_duration_ms":0,"hysteresis":5.0,"target":"disk:a","enabled":true}
            ]
        }"#;
    assert!(matches!(
        import_alert_rules_json(target_on_system_rule),
        Err(AlertRuleTransferError::InvalidRule {
            index: 0,
            field: "target",
            ..
        })
    ));
}

#[test]
fn alert_rule_merge_conflict_policies_are_atomic_and_ordered() {
    let existing = [
        transfer_entry("cpu", 80.0, true),
        transfer_entry("memory", 70.0, true),
    ];
    let imported = [
        transfer_entry("cpu", 95.0, false),
        transfer_entry("new", 60.0, true),
    ];

    assert_eq!(
        merge_alert_rule_entries(&existing, &imported, AlertRuleConflictPolicy::Reject),
        Err(AlertRuleTransferError::Conflict("cpu".into()))
    );

    let kept =
        merge_alert_rule_entries(&existing, &imported, AlertRuleConflictPolicy::KeepExisting)
            .unwrap();
    assert_eq!(kept.added, 1);
    assert_eq!(kept.replaced, 0);
    assert_eq!(kept.kept_existing, 1);
    assert_eq!(kept.entries[0], existing[0]);
    assert_eq!(kept.entries[2].rule.id, "new");

    let replaced = merge_alert_rule_entries(
        &existing,
        &imported,
        AlertRuleConflictPolicy::ReplaceExisting,
    )
    .unwrap();
    assert_eq!(replaced.added, 1);
    assert_eq!(replaced.replaced, 1);
    assert_eq!(replaced.kept_existing, 0);
    assert_eq!(replaced.entries[0], imported[0]);
    assert_eq!(replaced.entries[1], existing[1]);
    assert_eq!(replaced.entries[2], imported[1]);
}

#[test]
fn alert_rule_export_rejects_non_finite_and_duplicate_data() {
    let mut invalid = transfer_entry("cpu", 80.0, true);
    invalid.rule.threshold = f32::NAN;
    assert!(matches!(
        export_alert_rules_json(&[invalid]),
        Err(AlertRuleTransferError::InvalidRule {
            field: "threshold",
            ..
        })
    ));

    let duplicate = transfer_entry("cpu", 80.0, true);
    assert_eq!(
        export_alert_rules_json(&[duplicate.clone(), duplicate]),
        Err(AlertRuleTransferError::DuplicateRuleId("cpu".into()))
    );
}
