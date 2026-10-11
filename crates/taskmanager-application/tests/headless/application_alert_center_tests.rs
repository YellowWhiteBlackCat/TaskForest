use super::*;
use crate::AlertRuleImportMode;
use std::time::Duration;
use taskmanager_core::alerts::{
    AlertEventKind, AlertInstanceId, AlertMetric, AlertRule, AlertRuleId, AlertSeverity,
    AlertTargetIdentity, MAX_ALERT_EVENTS,
};
use taskmanager_core::core::DeviceGeneration;
use taskmanager_core::core::device_state::{DeviceLifecycle, DevicePresence};
use taskmanager_core::core::storage::StorageIdentityStability;
use taskmanager_core::metrics::{
    CpuMetrics, CpuScalarObservations, DiskMetrics, MemoryMetrics, ScalarObservation,
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
    let observed = snapshot(95.0);
    let first = center.evaluate(&observed, 1_000);
    assert_eq!(first.active.len(), 1);
    assert!(first.notifications.is_empty(), "notifications are opt-in");
    assert!(first.changes.active);
    assert!(first.changes.events);

    // Same threshold state on the next pass: active stays, no new fire.
    let repeat = center.evaluate(&observed, 2_000);
    assert_eq!(repeat.active.len(), 1);
    assert!(repeat.notifications.is_empty());
    assert_eq!(repeat.changes, AlertProjectionChanges::default());
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
fn event_history_records_only_active_set_transitions() {
    let mut center = AlertCenter::new([cpu_rule(90.0)]);

    center.evaluate(&snapshot(95.0), 1_000);
    center.evaluate(&snapshot(96.0), 2_000);
    center.evaluate(&snapshot(10.0), 3_000);

    let history = center.event_history();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].id.get(), 1);
    assert_eq!(history[0].kind, AlertEventKind::Activated);
    assert_eq!(history[1].id.get(), 2);
    assert_eq!(history[1].kind, AlertEventKind::Cleared);
    assert_eq!(
        history[1].alert.instance_id,
        AlertInstanceId::new(AlertRuleId::new("cpu-high"), AlertTargetIdentity::System)
    );
}

#[test]
fn clearing_event_history_does_not_reset_alert_evaluation() {
    let mut center = AlertCenter::new([cpu_rule(90.0)]);
    center.evaluate(&snapshot(95.0), 1_000);
    assert_eq!(center.event_history().len(), 1);

    center.clear_event_history();
    assert!(center.event_history().is_empty());
    assert_eq!(center.evaluate(&snapshot(96.0), 2_000).active.len(), 1);
    assert!(center.event_history().is_empty());
}

#[test]
fn event_history_is_bounded_and_keeps_transition_order() {
    let mut center = AlertCenter::new([cpu_rule(90.0)]);
    for index in 0..(MAX_ALERT_EVENTS + 3) {
        let base = index as u64 * 2_000;
        center.evaluate(&snapshot(95.0), base + 1_000);
        center.evaluate(&snapshot(10.0), base + 2_000);
    }

    let history = center.event_history();
    assert_eq!(history.len(), MAX_ALERT_EVENTS);
    assert_eq!(history.last().map(|event| event.id.get()), Some(206));
    assert!(history.windows(2).all(|events| events[0].id < events[1].id));
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
        AlertRuleEditResult {
            outcome: ManagedAlertRuleEditOutcome::Applied,
            changes: AlertProjectionChanges {
                rules: true,
                ..AlertProjectionChanges::default()
            },
        }
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
    assert_eq!(center.managed_rules()[0].rule.id.as_str(), "memory-high");
    assert_eq!(
        center
            .edit_rules(ManagedAlertRuleEdit::Toggle {
                rule_id: "cpu-high".into(),
            })
            .unwrap(),
        AlertRuleEditResult {
            outcome: ManagedAlertRuleEditOutcome::MissingTarget,
            changes: AlertProjectionChanges::default(),
        },
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
            .unwrap()
            .outcome,
        ManagedAlertRuleEditOutcome::Applied
    );
    assert_eq!(center.managed_rules()[0].rule.id.as_str(), "cpu-renamed");

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

#[test]
fn clear_replace_reset_and_event_eviction_keep_event_ids_monotonic() {
    let mut center = AlertCenter::new([cpu_rule(90.0)]);
    let active = snapshot(95.0);
    center.evaluate(&active, 1_000);
    assert_eq!(center.event_history()[0].id.get(), 1);

    let seed_alert = center.active_alerts()[0].clone();
    let seed = || AlertEventSeed {
        kind: AlertEventKind::Activated,
        alert: seed_alert.clone(),
        observed_at_ms: 5,
    };
    let replaced = center.replace_event_history(vec![seed(), seed()]);
    assert!(replaced.events);
    assert_eq!(center.event_history()[0].id.get(), 2);
    assert_eq!(center.event_history()[1].id.get(), 3);

    let cleared = center.clear_event_history();
    assert_eq!(cleared.events, true);
    assert!(center.event_history().is_empty());
    assert_eq!(
        center.clear_event_history(),
        AlertProjectionChanges::default()
    );

    // A reset clears the active projection while preserving the event high-water.
    let reset = center.reset();
    assert!(reset.active);
    assert!(!reset.events);
    center.evaluate(&active, 1_000);
    assert_eq!(center.event_history()[0].id.get(), 4);

    let seeds = (0..MAX_ALERT_EVENTS + 3)
        .map(|_| seed())
        .collect::<Vec<_>>();
    center.replace_event_history(seeds);
    assert_eq!(center.event_history().len(), MAX_ALERT_EVENTS);
    assert_eq!(
        center.event_history().first().map(|event| event.id.get()),
        Some(8)
    );
    assert_eq!(
        center.event_history().last().map(|event| event.id.get()),
        Some(107)
    );

    // Event append ordering is its sequence, even when observed times regress.
    let mut later_clear = snapshot(10.0);
    later_clear.timestamp_ms = 200;
    center.evaluate(&later_clear, 1_500);
    let mut earlier_activation = active;
    earlier_activation.timestamp_ms = 100;
    center.evaluate(&earlier_activation, 1_000);
    let tail = &center.event_history()[MAX_ALERT_EVENTS - 2..];
    assert!(tail[0].observed_at_ms > tail[1].observed_at_ms);
    assert_eq!(tail[0].id.get() + 1, tail[1].id.get());
}

#[test]
fn rule_edit_signals_only_actual_rule_projection_changes() {
    let mut center = AlertCenter::new([cpu_rule(90.0)]);
    let unchanged = center
        .edit_rules(ManagedAlertRuleEdit::Update {
            target_id: AlertRuleId::new("cpu-high"),
            managed: ManagedAlertRule::new(cpu_rule(90.0), true),
        })
        .unwrap();
    assert_eq!(unchanged.outcome, ManagedAlertRuleEditOutcome::Unchanged);
    assert_eq!(unchanged.changes, AlertProjectionChanges::default());

    let missing = center
        .edit_rules(ManagedAlertRuleEdit::Remove {
            rule_id: AlertRuleId::new("missing"),
        })
        .unwrap();
    assert_eq!(missing.outcome, ManagedAlertRuleEditOutcome::MissingTarget);
    assert_eq!(missing.changes, AlertProjectionChanges::default());

    let applied = center
        .edit_rules(ManagedAlertRuleEdit::Update {
            target_id: AlertRuleId::new("cpu-high"),
            managed: ManagedAlertRule::new(cpu_rule(80.0), true),
        })
        .unwrap();
    assert_eq!(applied.outcome, ManagedAlertRuleEditOutcome::Applied);
    assert!(applied.changes.rules);
    assert!(!applied.changes.active);
    assert!(!applied.changes.events);

    let invalid = center.edit_rules(ManagedAlertRuleEdit::Update {
        target_id: AlertRuleId::new("cpu-high"),
        managed: ManagedAlertRule::new(cpu_rule(f32::NAN), true),
    });
    assert!(invalid.is_err());
}

#[test]
fn untrusted_disk_refresh_retires_the_previous_active_identity_and_cooldown() {
    let disk_rule = AlertRule::new(
        "disk-hot",
        AlertMetric::SmartPercentUsed,
        AlertSeverity::Warning,
        80.0,
        Duration::ZERO,
        0.0,
    );
    let mut center = AlertCenter::new([disk_rule]);
    center.set_policy(NotificationPolicy {
        enabled: true,
        cooldown_ms: 100_000,
        ..NotificationPolicy::default()
    });

    let disk_snapshot = |timestamp_ms, identity_stability| {
        let mut disk = DiskMetrics::new("System Disk");
        disk.device_id = "wwid-1".into();
        disk.device_generation = DeviceGeneration::INITIAL;
        disk.identity_stability = identity_stability;
        disk.smart_percent_used = Some(90.0);
        SystemSnapshot {
            timestamp_ms,
            disks: vec![disk],
            device_lifecycles: std::collections::HashMap::from([(
                "wwid-1".into(),
                DeviceLifecycle {
                    presence: DevicePresence::Present,
                    generation: DeviceGeneration::INITIAL,
                    ..DeviceLifecycle::default()
                },
            )]),
            ..SystemSnapshot::default()
        }
    };

    let trusted = center.evaluate(
        &disk_snapshot(1_000, StorageIdentityStability::Persistent),
        1_000,
    );
    assert_eq!(trusted.active.len(), 1);
    assert_eq!(trusted.notifications.len(), 1);

    let untrusted = center.evaluate(
        &disk_snapshot(2_000, StorageIdentityStability::Unknown),
        2_000,
    );
    assert!(untrusted.active.is_empty());
    assert!(untrusted.changes.active);
    assert!(untrusted.changes.events);

    let recovered = center.evaluate(
        &disk_snapshot(3_000, StorageIdentityStability::Persistent),
        3_000,
    );
    assert_eq!(recovered.active.len(), 1);
    assert_eq!(recovered.notifications.len(), 1);
}

#[test]
fn exhausted_event_sequence_never_wraps_or_replaces_with_reused_ids() {
    let mut center = AlertCenter::new([cpu_rule(90.0)]);
    center.evaluate(&snapshot(95.0), 1_000);
    assert_eq!(center.event_history()[0].id.get(), 1);
    let existing = center.event_history().to_vec();
    center.event_high_water = u64::MAX;

    let seed = AlertEventSeed {
        kind: AlertEventKind::Cleared,
        alert: center.active_alerts()[0].clone(),
        observed_at_ms: 2_000,
    };
    assert_eq!(
        center.replace_event_history(vec![seed]),
        AlertProjectionChanges::default()
    );
    assert_eq!(center.event_history(), existing);

    let cleared = center.evaluate(&snapshot(10.0), 2_000);
    assert!(cleared.changes.active);
    assert!(!cleared.changes.events);
    let refired = center.evaluate(&snapshot(95.0), 3_000);
    assert!(refired.changes.active);
    assert!(!refired.changes.events);
    assert_eq!(center.event_history(), existing);
    assert_eq!(center.event_high_water, u64::MAX);
}
