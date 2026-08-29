use std::time::Duration;

use taskmanager_core::core::{AlertMetric, AlertRule, AlertSeverity};

use super::{
    RuleAdjustment, apply_adjustment, maximum_threshold, next_custom_rule_id, target_options,
};

#[test]
fn duration_hysteresis_and_target_edits_are_bounded_and_reversible() {
    let targets = target_options(&[
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .device_id("disk:a".into())
            .name("nvme0n1".into())
            .build(),
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .device_id("disk:b".into())
            .name("sda".into())
            .build(),
    ]);
    let mut rule = AlertRule::new(
        "disk-hot",
        AlertMetric::DiskTemperatureC,
        AlertSeverity::Warning,
        70.0,
        Duration::from_secs(5),
        5.0,
    );
    assert!(apply_adjustment(
        &mut rule,
        RuleAdjustment::Duration(-5),
        &targets
    ));
    assert_eq!(rule.for_duration, Duration::ZERO);
    assert!(apply_adjustment(
        &mut rule,
        RuleAdjustment::Hysteresis(-10.0),
        &targets
    ));
    assert_eq!(rule.hysteresis, 0.0);
    for expected in [Some("disk:a"), Some("disk:b"), None] {
        assert!(apply_adjustment(
            &mut rule,
            RuleAdjustment::Target,
            &targets
        ));
        assert_eq!(rule.target.as_deref(), expected);
    }
    let mut cpu_rule = AlertRule::new(
        "cpu",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Info,
        80.0,
        Duration::ZERO,
        5.0,
    )
    .for_target("stale-disk");
    assert!(apply_adjustment(
        &mut cpu_rule,
        RuleAdjustment::Target,
        &targets
    ));
    assert!(cpu_rule.target.is_none());
}

#[test]
fn editable_thresholds_match_transfer_contract_and_custom_ids_stay_unique() {
    let mut cpu_rule = AlertRule::new(
        "cpu",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Info,
        100.0,
        Duration::ZERO,
        5.0,
    );
    assert!(!apply_adjustment(
        &mut cpu_rule,
        RuleAdjustment::Threshold(5.0),
        &[],
    ));
    assert_eq!(cpu_rule.threshold, maximum_threshold(cpu_rule.metric));

    let rules = [
        super::ManagedAlertRule {
            rule: cpu_rule,
            enabled: true,
        },
        super::ManagedAlertRule {
            rule: AlertRule::new(
                "custom-2",
                AlertMetric::CpuUsagePercent,
                AlertSeverity::Info,
                80.0,
                Duration::ZERO,
                5.0,
            ),
            enabled: true,
        },
    ];
    assert_eq!(next_custom_rule_id(&rules), "custom-1");
}
