use std::time::Duration;

use taskforest_core::core::SystemSnapshot;
use taskforest_core::core::alerts::{
    Alert, AlertEventKind, AlertInstanceId, AlertMetric, AlertRule, AlertRuleId, AlertSeverity,
    AlertTargetIdentity,
};
use taskforest_core::core::metrics::{CpuMetrics, CpuScalarObservations, ScalarObservation};
use taskmanager_application::{
    AlertEventSeed, AlertRuleImportMode, ManagedAlertRule, ManagedAlertRuleEdit,
};

use crate::app::ShellApp;
use crate::app::alert_rows::{AlertRowFamily, AlertRowId};

fn instant_cpu_rule() -> AlertRule {
    AlertRule::new(
        "cpu-high",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Warning,
        90.0,
        Duration::ZERO,
        0.0,
    )
}

fn snapshot_with_cpu(usage: f32, timestamp_ms: u64) -> SystemSnapshot {
    SystemSnapshot {
        timestamp_ms,
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(usage, timestamp_ms),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn seed(rule_id: &str, kind: AlertEventKind, observed_at_ms: u64) -> AlertEventSeed {
    let rule_id = AlertRuleId::new(rule_id);
    AlertEventSeed {
        kind,
        alert: Alert {
            instance_id: AlertInstanceId::new(rule_id.clone(), AlertTargetIdentity::System),
            rule_id,
            target: "CPU".into(),
            metric: AlertMetric::CpuUsagePercent,
            severity: AlertSeverity::Warning,
            value: 95.0,
            threshold: 90.0,
            active_since_ms: observed_at_ms,
        },
        observed_at_ms,
    }
}

#[test]
fn rule_active_and_event_rows_have_separate_revisions_and_stale_anchors_fail_closed() {
    let mut app = ShellApp::new();
    let initial_rules = app
        .projection()
        .alert_row_projection_generation(AlertRowFamily::Rules);
    let initial_active = app
        .projection()
        .alert_row_projection_generation(AlertRowFamily::Active);
    let initial_events = app
        .projection()
        .alert_row_projection_generation(AlertRowFamily::Events);

    app.edit_alert_rules(ManagedAlertRuleEdit::Import {
        rules: vec![ManagedAlertRule::new(instant_cpu_rule(), true)],
        mode: AlertRuleImportMode::Replace,
    })
    .expect("rule import is valid");

    let rules = app
        .projection()
        .alert_row_projection_generation(AlertRowFamily::Rules);
    assert_eq!(
        rules.data_revision().get(),
        initial_rules.data_revision().get() + 1
    );
    assert_eq!(
        rules.order_revision().get(),
        initial_rules.order_revision().get() + 1
    );
    assert_eq!(
        app.projection()
            .alert_row_projection_generation(AlertRowFamily::Active),
        initial_active
    );
    assert_eq!(
        app.projection()
            .alert_row_projection_generation(AlertRowFamily::Events),
        initial_events
    );
    assert_eq!(
        app.projection().alert_row_ids(AlertRowFamily::Rules),
        vec![AlertRowId::Rule(AlertRuleId::new("cpu-high"))]
    );
    let stale_rule = app
        .projection()
        .alert_row_anchor_at(AlertRowFamily::Rules, 0)
        .expect("one managed rule row");

    let high = snapshot_with_cpu(95.0, 1);
    let activation = app.evaluate_alerts(&high, 1);
    assert!(activation.changes.active);
    assert!(activation.changes.events);
    assert_eq!(
        app.projection().alert_row_ids(AlertRowFamily::Active).len(),
        1
    );
    assert_eq!(
        app.projection().alert_row_ids(AlertRowFamily::Events).len(),
        1
    );
    assert!(
        app.projection()
            .resolve_alert_row_anchor(&stale_rule)
            .is_some()
    );

    let active_generation = app
        .projection()
        .alert_row_projection_generation(AlertRowFamily::Active);
    let event_generation = app
        .projection()
        .alert_row_projection_generation(AlertRowFamily::Events);
    let stale_event = app
        .projection()
        .alert_row_anchor_at(AlertRowFamily::Events, 0)
        .expect("activation transition row");

    let same = app.evaluate_alerts(&high, 1);
    assert!(!same.changes.active);
    assert!(!same.changes.events);
    assert_eq!(
        app.projection()
            .alert_row_projection_generation(AlertRowFamily::Active),
        active_generation
    );
    assert_eq!(
        app.projection()
            .alert_row_projection_generation(AlertRowFamily::Events),
        event_generation
    );

    let changed_value = app.evaluate_alerts(&snapshot_with_cpu(96.0, 2), 2);
    assert!(changed_value.changes.active);
    assert!(!changed_value.changes.events);
    assert!(
        app.projection()
            .resolve_alert_row_anchor(&stale_event)
            .is_some()
    );

    let cleared = app.evaluate_alerts(&snapshot_with_cpu(0.0, 3), 3);
    assert!(cleared.changes.active);
    assert!(cleared.changes.events);
    assert!(
        app.projection()
            .alert_row_ids(AlertRowFamily::Active)
            .is_empty()
    );
    assert!(
        app.projection()
            .resolve_alert_row_anchor(&stale_event)
            .is_none()
    );
    assert!(
        app.projection()
            .resolve_alert_row_anchor(&stale_rule)
            .is_some()
    );

    let prior_last_id = app
        .projection()
        .alert_center
        .event_history()
        .last()
        .expect("activation and clear transitions")
        .id;
    app.clear_alert_event_history();
    assert!(
        app.projection()
            .alert_row_ids(AlertRowFamily::Events)
            .is_empty()
    );
    app.replace_alert_event_history(vec![seed("cpu-high", AlertEventKind::Activated, 0)]);
    let replacement = app
        .projection()
        .alert_center
        .event_history()
        .last()
        .expect("replacement seed receives an owner ID")
        .id;
    assert!(replacement > prior_last_id);
}

#[test]
fn unrelated_batches_do_not_advance_alert_row_revisions() {
    let mut app = ShellApp::new();
    let before = [
        app.projection()
            .alert_row_projection_generation(AlertRowFamily::Rules),
        app.projection()
            .alert_row_projection_generation(AlertRowFamily::Active),
        app.projection()
            .alert_row_projection_generation(AlertRowFamily::Events),
    ];

    app.apply_platform_batch(Default::default());

    assert_eq!(
        before,
        [
            app.projection()
                .alert_row_projection_generation(AlertRowFamily::Rules),
            app.projection()
                .alert_row_projection_generation(AlertRowFamily::Active),
            app.projection()
                .alert_row_projection_generation(AlertRowFamily::Events),
        ]
    );
}

#[test]
fn first_same_identity_rule_update_advances_data_without_claiming_reorder() {
    let mut app = ShellApp::new();
    let before = app
        .projection()
        .alert_row_projection_generation(AlertRowFamily::Rules);
    assert!(
        app.projection()
            .alert_row_ids(AlertRowFamily::Rules)
            .contains(&AlertRowId::Rule(AlertRuleId::new("cpu-high")))
    );

    app.edit_alert_rules(ManagedAlertRuleEdit::Update {
        target_id: AlertRuleId::new("cpu-high"),
        managed: ManagedAlertRule::new(
            AlertRule::new(
                "cpu-high",
                AlertMetric::CpuUsagePercent,
                AlertSeverity::Warning,
                85.0,
                Duration::ZERO,
                0.0,
            ),
            true,
        ),
    })
    .expect("same-ID threshold update is valid");

    let after = app
        .projection()
        .alert_row_projection_generation(AlertRowFamily::Rules);
    assert_eq!(
        after.data_revision().get(),
        before.data_revision().get() + 1
    );
    assert_eq!(after.order_revision(), before.order_revision());
}
