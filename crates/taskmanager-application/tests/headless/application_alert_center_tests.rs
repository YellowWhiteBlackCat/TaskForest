use super::*;
use crate::AlertRuleImportMode;
use std::time::Duration;
use taskmanager_core::alerts::{AlertMetric, AlertRule, AlertSeverity};
use taskmanager_core::metrics::{
    CpuMetrics, CpuScalarObservations, MemoryMetrics, ScalarObservation,
};

fn cpu_rule(threshold: f32) -> AlertRule {
    AlertRule::new(
        "cpu-high",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Warning,
        threshold,
        Duration::ZERO,
        0.0,
    )
}

fn snapshot(cpu: f32) -> SystemSnapshot {
    SystemSnapshot {
        timestamp_ms: 1_000,
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(cpu, 1_000),
            ..Default::default()
        }),
        memory: MemoryMetrics::default(),
        ..Default::default()
    }
}

#[test]
fn evaluate_reports_active_and_quiet_repeats() {
    let mut center = AlertCenter::new([cpu_rule(90.0)]);
    let first = center.evaluate(&snapshot(95.0), 1_000);
    assert_eq!(first.active.len(), 1);
    assert!(first.notifications.is_empty(), "notifications are opt-in");

    // Same threshold state on the next pass: active stays, no new fire.
    let repeat = center.evaluate(&snapshot(96.0), 2_000);
    assert_eq!(repeat.active.len(), 1);
    assert!(repeat.notifications.is_empty());
}

#[test]
fn evaluate_tracks_clear_and_refire() {
    let mut center = AlertCenter::new([cpu_rule(90.0)]);
    center.set_policy(NotificationPolicy {
        enabled: true,
        cooldown_ms: 0,
        ..NotificationPolicy::default()
    });
    assert_eq!(
        center.evaluate(&snapshot(95.0), 1_000).notifications.len(),
        1
    );
    // Cleared below threshold, then re-fired: a NEW transition.
    let cleared = center.evaluate(&snapshot(10.0), 2_000);
    assert!(cleared.active.is_empty());
    let refired = center.evaluate(&snapshot(95.0), 3_000);
    assert_eq!(refired.notifications.len(), 1);
}

#[test]
fn default_center_builds_product_rules() {
    let center = AlertCenter::default();
    assert_eq!(center.managed_rules().len(), default_rules().len());
    assert!(center.managed_rules().iter().all(|managed| managed.enabled));
}

#[test]
fn replace_import_rebuilds_enabled_projection_without_touching_policy() {
    let mut center = AlertCenter::new([cpu_rule(90.0)]);
    center.set_policy(NotificationPolicy {
        enabled: true,
        cooldown_ms: 0,
        ..NotificationPolicy::default()
    });
    center
        .edit_rules(ManagedAlertRuleEdit::Import {
            rules: vec![ManagedAlertRule::new(cpu_rule(10.0), true)],
            mode: AlertRuleImportMode::Replace,
        })
        .unwrap();
    let evaluation = center.evaluate(&snapshot(50.0), 1_000);
    assert_eq!(evaluation.active.len(), 1);
    assert_eq!(evaluation.notifications.len(), 1);
    assert!(center.policy().enabled);
}

#[test]
fn disabled_rule_stays_listed_but_cannot_become_active() {
    let mut center = AlertCenter::new([cpu_rule(10.0)]);
    assert_eq!(
        center
            .edit_rules(ManagedAlertRuleEdit::Toggle {
                rule_id: "cpu-high".into(),
            })
            .unwrap(),
        ManagedAlertRuleEditOutcome::Applied
    );

    assert_eq!(center.managed_rules().len(), 1);
    assert!(!center.managed_rules()[0].enabled);
    assert!(center.enabled_rules().is_empty());
    assert!(center.evaluate(&snapshot(95.0), 1_000).active.is_empty());
}

#[test]
fn merge_replace_and_missing_stable_target_have_atomic_semantics() {
    use taskmanager_core::alerts::AlertRuleConflictPolicy;

    let mut center = AlertCenter::new([cpu_rule(90.0)]);
    let memory = AlertRule::new(
        "memory-high",
        AlertMetric::MemoryUsagePercent,
        AlertSeverity::Warning,
        80.0,
        Duration::ZERO,
        5.0,
    );
    center
        .edit_rules(ManagedAlertRuleEdit::Import {
            rules: vec![ManagedAlertRule::new(memory, false)],
            mode: AlertRuleImportMode::Merge(AlertRuleConflictPolicy::ReplaceExisting),
        })
        .unwrap();
    assert_eq!(center.managed_rules().len(), 2);
    assert!(!center.managed_rules()[1].enabled);

    center
        .edit_rules(ManagedAlertRuleEdit::Remove {
            rule_id: "cpu-high".into(),
        })
        .unwrap();
    assert_eq!(center.managed_rules()[0].rule.id, "memory-high");
    assert_eq!(
        center
            .edit_rules(ManagedAlertRuleEdit::Toggle {
                rule_id: "cpu-high".into(),
            })
            .unwrap(),
        ManagedAlertRuleEditOutcome::MissingTarget,
        "a deleted target cannot toggle the rule that moved into its old position"
    );
    assert!(!center.managed_rules()[0].enabled);

    center
        .edit_rules(ManagedAlertRuleEdit::Import {
            rules: vec![ManagedAlertRule::new(cpu_rule(50.0), true)],
            mode: AlertRuleImportMode::Replace,
        })
        .unwrap();
    assert_eq!(center.managed_rules().len(), 1);
    assert_eq!(center.managed_rules()[0].rule.threshold, 50.0);
}

#[test]
fn update_targets_stable_identity_and_validates_a_replacement_id_atomically() {
    let mut center = AlertCenter::new([
        cpu_rule(90.0),
        AlertRule::new(
            "memory-high",
            AlertMetric::MemoryUsagePercent,
            AlertSeverity::Warning,
            80.0,
            Duration::ZERO,
            5.0,
        ),
    ]);
    let renamed = ManagedAlertRule::new(
        AlertRule::new(
            "cpu-renamed",
            AlertMetric::CpuUsagePercent,
            AlertSeverity::Critical,
            70.0,
            Duration::ZERO,
            5.0,
        ),
        true,
    );
    assert_eq!(
        center
            .edit_rules(ManagedAlertRuleEdit::Update {
                target_id: "cpu-high".into(),
                managed: renamed,
            })
            .unwrap(),
        ManagedAlertRuleEditOutcome::Applied
    );
    assert_eq!(center.managed_rules()[0].rule.id, "cpu-renamed");

    let before = center.managed_rules().to_vec();
    let duplicate = ManagedAlertRule::new(cpu_rule(60.0), true);
    let mut duplicate = duplicate;
    duplicate.rule.id = "memory-high".into();
    assert!(
        center
            .edit_rules(ManagedAlertRuleEdit::Update {
                target_id: "cpu-renamed".into(),
                managed: duplicate,
            })
            .is_err()
    );
    assert_eq!(center.managed_rules(), before);
}
