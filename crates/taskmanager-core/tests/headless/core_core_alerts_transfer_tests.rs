use super::*;

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
fn json_v1_is_stable_and_round_trips_enabled_state() {
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
fn import_strictly_rejects_unknown_version_fields_and_bad_rules() {
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
fn merge_conflict_policies_are_atomic_and_ordered() {
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
fn export_rejects_non_finite_and_duplicate_data() {
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

#[test]
fn merging_past_the_rule_ceiling_fails_on_the_crossing_operation() {
    let fill = (0..crate::core::alerts::MAX_TRANSFER_RULES)
        .map(|index| transfer_entry(&format!("rule-{index}"), 90.0, true))
        .collect::<Vec<_>>();
    let one_more = transfer_entry("rule-overflow", 90.0, true);

    // Each side is inside the ceiling, but the merged result is not: the
    // operation that crosses the cap must be the one reporting TooManyRules,
    // so a later edit never freezes behind a pre-existing overflow.
    assert!(matches!(
        merge_alert_rule_entries(&fill, std::slice::from_ref(&one_more), AlertRuleConflictPolicy::Reject),
        Err(AlertRuleTransferError::TooManyRules(count)) if count == crate::core::alerts::MAX_TRANSFER_RULES + 1
    ));
    let disjoint = (0..crate::core::alerts::MAX_TRANSFER_RULES)
        .map(|index| transfer_entry(&format!("other-{index}"), 90.0, true))
        .collect::<Vec<_>>();
    assert!(matches!(
        merge_alert_rule_entries(&fill, &disjoint, AlertRuleConflictPolicy::KeepExisting),
        Err(AlertRuleTransferError::TooManyRules(count)) if count == 2 * crate::core::alerts::MAX_TRANSFER_RULES
    ));
    // Replacing an existing id keeps the collection at the cap and succeeds.
    assert!(
        merge_alert_rule_entries(
            &fill,
            std::slice::from_ref(&AlertRuleTransferEntry::new(
                AlertRule::new(
                    "rule-0",
                    AlertMetric::CpuUsagePercent,
                    AlertSeverity::Warning,
                    91.0,
                    Duration::from_millis(5_000),
                    5.0,
                ),
                true,
            )),
            AlertRuleConflictPolicy::ReplaceExisting,
        )
        .is_ok()
    );
}
