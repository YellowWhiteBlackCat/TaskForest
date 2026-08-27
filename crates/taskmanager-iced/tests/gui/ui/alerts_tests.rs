use super::*;
use crate::app::Message;
use taskmanager_application::alerts::{AlertMetric, AlertRule};
use taskmanager_application::i18n::{Language, set_language};

fn pin_english() {
    set_language(Language::En);
}

fn opened_demo() -> crate::IcedApp {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    app
}

#[test]
fn rule_rows_mirror_the_shared_rules_with_typed_current_values() {
    pin_english();
    let app = opened_demo();
    let rows = rule_rows(&app);

    assert_eq!(
        rows.len(),
        app.shell.projection().alert_center.managed_rules().len(),
        "one row per shell rule"
    );
    assert_eq!(rows[0].metric_label, "CPU usage");
    assert_eq!(rows[0].threshold_text, "90.0%");
    // The demo snapshot's honest CPU observation (37.4%), not a zero.
    assert_eq!(rows[0].current_text, "37.4%");
    // Memory is derived from the fixture's used/total bytes (~39.5%).
    assert_eq!(rows[1].metric_label, "Memory usage");
    assert_eq!(rows[1].current_text, "39.5%");
    // Disk-family rules show their scope, not an ambiguous disk value.
    assert_eq!(rows[2].metric_label, "Disk temperature");
    assert_eq!(rows[2].current_text, "All disks");
    assert!(rows.iter().all(|row| row.enabled));
}

#[test]
fn unobserved_metrics_render_none_never_zero() {
    pin_english();
    let mut app = crate::IcedApp::default();
    // No snapshot at all: the typed accessors are absent, so the rows
    // must carry the localized None marker instead of a fabricated 0.
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    let rows = rule_rows(&app);
    assert_eq!(rows[0].current_text, "None");
    assert!(
        !rows.iter().any(|row| row.current_text == "0.0%"),
        "no fabricated zero may render for an unobserved metric"
    );
}

#[test]
fn toggling_flips_the_row_switch_state() {
    pin_english();
    let mut app = opened_demo();
    assert!(rule_rows(&app)[0].enabled);
    let _ = app.update(Message::Alerts(AlertsMessage::ToggleRule {
        rule_id: "cpu-high".into(),
    }));
    assert!(!rule_rows(&app)[0].enabled);
}

#[test]
fn active_alert_lines_name_the_metric_value_and_severity() {
    pin_english();
    let mut app = crate::IcedApp::demo();
    // One zero-duration CPU rule at 30% (below the fixture's 37.4%) so a
    // single evaluation fires.
    app.shell
        .edit_alert_rules(taskmanager_application::ManagedAlertRuleEdit::Import {
            rules: vec![taskmanager_application::ManagedAlertRule::new(
                AlertRule::new(
                    "cpu-hot",
                    AlertMetric::CpuUsagePercent,
                    AlertSeverity::Warning,
                    30.0,
                    std::time::Duration::ZERO,
                    5.0,
                ),
                true,
            )],
            mode: taskmanager_application::AlertRuleImportMode::Replace,
        })
        .unwrap();
    let snapshot = app
        .shell
        .projection()
        .snapshot
        .clone()
        .expect("demo snapshot fixture");
    let evaluation = app.shell.evaluate_alerts(&snapshot, snapshot.timestamp_ms);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::ActiveAlerts(evaluation.active),
    );
    assert_eq!(app.shell.projection().alert_active.len(), 1);

    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    let lines = active_alert_lines(&app);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].severity, AlertSeverity::Warning);
    assert!(
        lines[0].text.contains("CPU usage"),
        "the banner must name the rule metric: {}",
        lines[0].text
    );
    assert!(lines[0].text.contains("37.4"));
    assert!(lines[0].text.contains("Warning"));
}

#[test]
fn an_empty_rule_set_renders_the_localized_empty_state() {
    pin_english();
    let mut app = crate::IcedApp::demo();
    app.shell
        .edit_alert_rules(taskmanager_application::ManagedAlertRuleEdit::Import {
            rules: Vec::new(),
            mode: taskmanager_application::AlertRuleImportMode::Replace,
        })
        .unwrap();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));

    assert!(rule_rows(&app).is_empty());
    assert_eq!(empty_state_text(), "No alert rules configured.");
}

#[test]
fn the_alerts_route_renders_in_the_root_view() {
    pin_english();
    let mut app = crate::IcedApp::demo();
    // Both route branches must construct the real element tree.
    let _ = crate::ui::view(&app);
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    assert!(app.alerts_page_open());
    let _ = crate::ui::view(&app);
}
