use super::*;
use crate::app::Message;
use taskmanager_core::core::alerts::AlertMetric;

#[test]
fn opening_the_page_reads_rows_from_the_shared_alert_center() {
    let mut app = crate::IcedApp::demo();
    assert!(!app.alerts_page_open());

    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));

    assert!(app.alerts_page_open());
    let shell_rules = app.shell.projection().alert_center.managed_rules();
    let managed = app.alerts_rules();
    assert_eq!(
        managed.len(),
        shell_rules.len(),
        "the page must render one row per shared rule"
    );
    assert_eq!(managed[0].rule.metric, AlertMetric::CpuUsagePercent);
    assert_eq!(managed[0].rule.threshold, 90.0);
    assert!(managed.iter().all(|row| row.enabled));
}

#[test]
fn toggling_a_rule_flips_the_shared_engine_membership_and_back() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    let before = app.shell.projection().alert_center.managed_rules().to_vec();

    let _ = app.update(Message::Alerts(AlertsMessage::ToggleRule {
        rule_id: "cpu-high".into(),
    }));

    assert!(!app.alerts_rules()[0].enabled, "the row toggle flips");
    let after = app.shell.projection().alert_center.managed_rules();
    assert_eq!(after.len(), before.len());
    assert!(
        after
            .iter()
            .any(|managed| { managed.rule.id == "cpu-high" && !managed.enabled }),
        "the disabled rule stays in the canonical managed list"
    );
    assert_eq!(
        app.shell.projection().alert_center.enabled_rules().len(),
        before.len() - 1
    );

    let _ = app.update(Message::Alerts(AlertsMessage::ToggleRule {
        rule_id: "cpu-high".into(),
    }));
    assert!(app.alerts_rules()[0].enabled);
    assert_eq!(
        app.shell.projection().alert_center.enabled_rules().len(),
        before.len()
    );
}

#[test]
fn a_missing_stable_toggle_target_is_an_honest_no_op() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    let before = app.shell.projection().alert_center.managed_rules().to_vec();

    let _ = app.update(Message::Alerts(AlertsMessage::ToggleRule {
        rule_id: "removed-rule".into(),
    }));

    assert_eq!(app.shell.projection().alert_center.managed_rules(), before);
}

#[test]
fn reopen_reads_the_same_canonical_enable_choices() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    let _ = app.update(Message::Alerts(AlertsMessage::ToggleRule {
        rule_id: "memory-high".into(),
    }));

    let _ = app.update(Message::Alerts(AlertsMessage::ClosePage));
    assert!(!app.alerts_page_open());
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));

    assert!(!app.alerts_rules()[1].enabled, "choices survive a close");
    assert_eq!(app.alerts_rules().len(), 5, "the managed list is durable");
    assert_eq!(
        app.shell.projection().alert_center.enabled_rules().len(),
        4,
        "the engine still holds only the enabled subset"
    );
}

#[test]
fn selecting_a_shared_page_closes_the_alerts_route() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));

    let _ = app.update(Message::SelectPage(
        taskmanager_application::AppPage::System,
    ));

    assert!(!app.alerts_page_open());
}

#[test]
fn escape_closes_the_alerts_route_when_no_modal_is_open() {
    use crate::keys::IcedKey;
    use taskmanager_application::{KeyCode, Modifiers};

    use taskmanager_shell::ShellKeyEvent;

    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));

    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::Escape,
        Modifiers::NONE,
    ))));

    assert!(!app.alerts_page_open());
}

#[test]
fn escape_still_closes_the_alerts_route_over_a_shell_page() {
    // The Escape branch must not be shadowed by the shared-page state:
    // with the alerts page closed, Escape keeps its shared no-op shape
    // (nothing to dismiss), and the route stays closed.
    use crate::keys::IcedKey;
    use taskmanager_application::{KeyCode, Modifiers};

    use taskmanager_shell::ShellKeyEvent;

    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    let _ = app.update(Message::Alerts(AlertsMessage::ClosePage));

    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::Escape,
        Modifiers::NONE,
    ))));

    assert!(!app.alerts_page_open());
}

#[test]
fn alt_eight_opens_the_alerts_route() {
    use crate::keys::IcedKey;
    use taskmanager_application::{KeyCode, Modifiers};

    use taskmanager_shell::ShellKeyEvent;

    let mut app = crate::IcedApp::demo();
    assert!(!app.alerts_page_open());

    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::Digit8,
        Modifiers::ALT,
    ))));

    assert!(
        app.alerts_page_open(),
        "the router-registered ShowAlerts chord must open the page"
    );
    assert!(
        !app.alerts_rules().is_empty(),
        "opening reads the canonical managed-rule projection"
    );

    // The chord is idempotent while the route is already open.
    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::Digit8,
        Modifiers::ALT,
    ))));
    assert!(app.alerts_page_open());
}

#[test]
fn alt_eight_does_not_open_the_route_beneath_a_modal() {
    use crate::keys::IcedKey;
    use taskmanager_application::{KeyCode, Modifiers};

    use taskmanager_shell::ShellKeyEvent;

    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::OpenSettings);
    assert!(app.settings_open());

    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::Digit8,
        Modifiers::ALT,
    ))));

    assert!(
        !app.alerts_page_open(),
        "a modal owns the keyboard; the page must not open beneath it"
    );
    assert!(app.settings_open(), "the modal stays untouched");
}

#[test]
fn alt_eight_is_inert_while_search_owns_the_keyboard() {
    use crate::keys::IcedKey;
    use taskmanager_application::{KeyCode, Modifiers};

    use taskmanager_shell::ShellKeyEvent;

    let mut app = crate::IcedApp::demo();
    app.shell.open_search();

    let _ = app.update(Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
        KeyCode::Digit8,
        Modifiers::ALT,
    ))));

    assert!(
        !app.alerts_page_open(),
        "the shared Show* chords are blocked while the search field owns the keyboard"
    );
}

#[test]
fn alerts_focus_targets_are_stable_and_row_bound() {
    use crate::app::FocusTarget;

    // The frontend-local tab rides the nav strip's focus traversal with a
    // stable, page-namespaced operation id (peer of `iced-page-tab-*`).
    assert_eq!(
        crate::focus::focus_id(FocusTarget::AlertsPageTab),
        "iced-alerts-page-tab"
    );
    // One stable id per rule-row toggle; indices never collide.
    assert_eq!(
        crate::focus::focus_id(FocusTarget::AlertsRuleToggle(0)),
        "iced-alerts-rule-toggle-0"
    );
    assert_ne!(
        crate::focus::focus_id(FocusTarget::AlertsRuleToggle(0)),
        crate::focus::focus_id(FocusTarget::AlertsRuleToggle(1))
    );
    assert_eq!(
        crate::focus::focus_id(FocusTarget::AlertsExport),
        "iced-alerts-export"
    );
    assert_eq!(
        crate::focus::focus_id(FocusTarget::AlertsImport),
        "iced-alerts-import"
    );
    // Both stops are part of the frozen registry the uniqueness gate
    // walks (FocusTarget::ALL).
    assert!(FocusTarget::ALL.contains(&FocusTarget::AlertsPageTab));
    assert!(FocusTarget::ALL.contains(&FocusTarget::AlertsRuleToggle(0)));
    assert!(FocusTarget::ALL.contains(&FocusTarget::AlertsExport));
    assert!(FocusTarget::ALL.contains(&FocusTarget::AlertsImport));
}

#[test]
fn focusing_an_alerts_stop_updates_the_tracked_control() {
    use crate::app::FocusTarget;

    // The Tab-cycle seam: a focused stop is tracked exactly like every
    // other registered control, so the focus-restore path can round-trip
    // alerts stops.
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));

    let _ = app.update(Message::Focus(FocusTarget::AlertsPageTab));
    assert_eq!(app.input.focused_control, Some(FocusTarget::AlertsPageTab));

    let _ = app.update(Message::Focus(FocusTarget::AlertsRuleToggle(0)));
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::AlertsRuleToggle(0))
    );
}

#[test]
fn export_alert_rules_returns_valid_json_matching_canonical_rules() {
    let app = crate::IcedApp::demo();
    let json = app.export_alert_rules().expect("export should succeed");
    let imported = taskmanager_core::core::alerts::import_alert_rules_json(&json)
        .expect("exported JSON must be valid");
    let current_rules = app.alerts_rules();
    assert_eq!(imported.len(), current_rules.len());
    for (entry, managed) in imported.iter().zip(current_rules.iter()) {
        assert_eq!(entry.rule.id, managed.rule.id);
        assert_eq!(entry.rule.metric, managed.rule.metric);
        assert_eq!(entry.rule.threshold, managed.rule.threshold);
        assert_eq!(entry.enabled, managed.enabled);
    }
}

#[test]
fn import_alert_rules_replaces_rules_correctly() {
    use taskmanager_application::AlertRuleImportMode;
    use taskmanager_core::core::alerts::{
        AlertMetric, AlertRule, AlertRuleTransferEntry, AlertSeverity, export_alert_rules_json,
    };

    let mut app = crate::IcedApp::demo();
    let new_rule = AlertRule::new(
        "custom-cpu-rule",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Critical,
        95.0,
        std::time::Duration::from_secs(5),
        2.0,
    );
    let entries = [AlertRuleTransferEntry::new(new_rule, true)];
    let json = export_alert_rules_json(&entries).expect("export entries");

    let outcome = app
        .import_alert_rules(&json, AlertRuleImportMode::Replace)
        .expect("import should succeed");
    assert_eq!(
        outcome,
        taskmanager_application::ManagedAlertRuleEditOutcome::Applied
    );

    let managed = app.alerts_rules();
    assert_eq!(managed.len(), 1);
    assert_eq!(managed[0].rule.id, "custom-cpu-rule");
    assert_eq!(managed[0].rule.threshold, 95.0);
    assert!(managed[0].enabled);
}

#[test]
fn import_alert_rules_merges_with_replace_existing_policy() {
    use taskmanager_application::AlertRuleImportMode;
    use taskmanager_core::core::alerts::{
        AlertMetric, AlertRule, AlertRuleConflictPolicy, AlertRuleTransferEntry, AlertSeverity,
        export_alert_rules_json,
    };

    let mut app = crate::IcedApp::demo();
    let initial_count = app.alerts_rules().len();

    let updated_cpu = AlertRule::new(
        "cpu-high",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Critical,
        75.0,
        std::time::Duration::from_secs(5),
        2.0,
    );
    let new_rule = AlertRule::new(
        "brand-new-rule",
        AlertMetric::MemoryUsagePercent,
        AlertSeverity::Warning,
        85.0,
        std::time::Duration::from_secs(10),
        3.0,
    );
    let entries = [
        AlertRuleTransferEntry::new(updated_cpu, false),
        AlertRuleTransferEntry::new(new_rule, true),
    ];
    let json = export_alert_rules_json(&entries).expect("export entries");

    let outcome = app
        .import_alert_rules(
            &json,
            AlertRuleImportMode::Merge(AlertRuleConflictPolicy::ReplaceExisting),
        )
        .expect("merge import should succeed");
    assert_eq!(
        outcome,
        taskmanager_application::ManagedAlertRuleEditOutcome::Applied
    );

    let managed = app.alerts_rules();
    assert_eq!(managed.len(), initial_count + 1);
    let cpu = managed.iter().find(|m| m.rule.id == "cpu-high").unwrap();
    assert_eq!(cpu.rule.threshold, 75.0);
    assert!(!cpu.enabled);
    let new_m = managed
        .iter()
        .find(|m| m.rule.id == "brand-new-rule")
        .unwrap();
    assert_eq!(new_m.rule.threshold, 85.0);
    assert!(new_m.enabled);
}

#[test]
fn import_alert_rules_rejects_invalid_json() {
    use taskmanager_application::AlertRuleImportMode;
    use taskmanager_core::core::alerts::AlertRuleTransferError;

    let mut app = crate::IcedApp::demo();
    let before = app.alerts_rules().to_vec();

    let result = app.import_alert_rules("not a json", AlertRuleImportMode::Replace);
    assert!(matches!(
        result,
        Err(AlertRuleTransferError::InvalidJson(_))
    ));
    assert_eq!(app.alerts_rules(), before);
}

#[test]
fn export_rules_message_reports_clipboard_notice() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    let _ = app.update(Message::Alerts(AlertsMessage::ExportRules));

    let notice = app.shell.feedback_notice().expect("notice was reported");
    assert_eq!(
        notice.source(),
        taskmanager_shell::FeedbackSource::Clipboard
    );
    assert_eq!(
        notice.severity(),
        taskmanager_shell::FeedbackSeverity::Success
    );
    assert!(app.shell.feedback_text().contains("Alert rules"));
}

#[test]
fn import_rules_message_applies_and_reports_notice() {
    use taskmanager_application::AlertRuleImportMode;
    use taskmanager_core::core::alerts::{
        AlertMetric, AlertRule, AlertRuleTransferEntry, AlertSeverity, export_alert_rules_json,
    };

    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));

    let new_rule = AlertRule::new(
        "imported-cpu",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Warning,
        60.0,
        std::time::Duration::from_secs(1),
        1.0,
    );
    let json = export_alert_rules_json(&[AlertRuleTransferEntry::new(new_rule, true)]).unwrap();

    let _ = app.update(Message::Alerts(AlertsMessage::ImportRules {
        json,
        mode: AlertRuleImportMode::Replace,
    }));

    assert_eq!(app.alerts_rules().len(), 1);
    assert_eq!(app.alerts_rules()[0].rule.id, "imported-cpu");

    let notice = app.shell.feedback_notice().expect("notice was reported");
    assert_eq!(
        notice.source(),
        taskmanager_shell::FeedbackSource::Clipboard
    );
    assert_eq!(
        notice.severity(),
        taskmanager_shell::FeedbackSeverity::Success
    );
    assert!(app.shell.feedback_text().contains("succeeded"));

    let _ = app.update(Message::Alerts(AlertsMessage::ImportRules {
        json: "invalid-json".into(),
        mode: AlertRuleImportMode::Replace,
    }));
    let err_notice = app.shell.feedback_notice().expect("error notice reported");
    assert_eq!(
        err_notice.severity(),
        taskmanager_shell::FeedbackSeverity::Error
    );
}

#[test]
fn add_alert_rule_appends_new_rule_and_enables_it() {
    use taskmanager_core::core::alerts::{AlertMetric, AlertRule, AlertSeverity};

    let mut app = crate::IcedApp::demo();
    let initial_count = app.alerts_rules().len();

    let rule = AlertRule::new(
        "author-custom-cpu",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Critical,
        92.0,
        std::time::Duration::from_secs(4),
        2.0,
    );
    let outcome = app.add_alert_rule(rule.clone()).expect("add succeeds");
    assert_eq!(
        outcome,
        taskmanager_application::ManagedAlertRuleEditOutcome::Applied
    );
    assert_eq!(app.alerts_rules().len(), initial_count + 1);
    let added = app
        .alerts_rules()
        .iter()
        .find(|m| m.rule.id == "author-custom-cpu")
        .expect("rule was added");
    assert_eq!(added.rule.threshold, 92.0);
    assert!(added.enabled);

    let dup_res = app.add_alert_rule(rule);
    assert!(matches!(
        dup_res,
        Err(taskmanager_core::core::alerts::AlertRuleTransferError::Conflict(_))
    ));
}

#[test]
fn update_alert_rule_modifies_existing_rule_and_preserves_enabled_state() {
    use taskmanager_core::core::alerts::{AlertMetric, AlertRule, AlertSeverity};

    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    let _ = app.update(Message::Alerts(AlertsMessage::ToggleRule {
        rule_id: "cpu-high".into(),
    }));
    assert!(
        !app.alerts_rules()
            .iter()
            .find(|m| m.rule.id == "cpu-high")
            .unwrap()
            .enabled
    );

    let updated = AlertRule::new(
        "cpu-high",
        AlertMetric::CpuUsagePercent,
        AlertSeverity::Warning,
        82.5,
        std::time::Duration::from_secs(6),
        3.0,
    );
    let outcome = app.update_alert_rule(updated).expect("update succeeds");
    assert_eq!(
        outcome,
        taskmanager_application::ManagedAlertRuleEditOutcome::Applied
    );
    let managed = app
        .alerts_rules()
        .iter()
        .find(|m| m.rule.id == "cpu-high")
        .unwrap();
    assert_eq!(managed.rule.threshold, 82.5);
    assert_eq!(managed.rule.severity, AlertSeverity::Warning);
    assert!(
        !managed.enabled,
        "enabled state must be preserved on update"
    );

    let non_existent = AlertRule::new(
        "non-existent-rule",
        AlertMetric::MemoryUsagePercent,
        AlertSeverity::Info,
        50.0,
        std::time::Duration::from_secs(1),
        1.0,
    );
    let missing_outcome = app
        .update_alert_rule(non_existent)
        .expect("update call succeeds");
    assert_eq!(
        missing_outcome,
        taskmanager_application::ManagedAlertRuleEditOutcome::MissingTarget
    );
}

#[test]
fn remove_alert_rule_deletes_rule_by_id() {
    let mut app = crate::IcedApp::demo();
    assert!(app.alerts_rules().iter().any(|m| m.rule.id == "cpu-high"));

    let outcome = app
        .remove_alert_rule("cpu-high".into())
        .expect("remove succeeds");
    assert_eq!(
        outcome,
        taskmanager_application::ManagedAlertRuleEditOutcome::Applied
    );
    assert!(app.alerts_rules().iter().all(|m| m.rule.id != "cpu-high"));

    let missing_outcome = app
        .remove_alert_rule("cpu-high".into())
        .expect("remove call succeeds");
    assert_eq!(
        missing_outcome,
        taskmanager_application::ManagedAlertRuleEditOutcome::MissingTarget
    );
}

#[test]
fn add_rule_message_applies_and_reports_notice() {
    use taskmanager_core::core::alerts::{AlertMetric, AlertRule, AlertSeverity};

    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));

    let rule = AlertRule::new(
        "msg-added-rule",
        AlertMetric::MemoryUsagePercent,
        AlertSeverity::Warning,
        70.0,
        std::time::Duration::from_secs(2),
        1.0,
    );
    let _ = app.update(Message::Alerts(AlertsMessage::AddRule {
        rule: rule.clone(),
    }));
    assert!(
        app.alerts_rules()
            .iter()
            .any(|m| m.rule.id == "msg-added-rule")
    );

    let notice = app.shell.feedback_notice().expect("success notice");
    assert_eq!(
        notice.source(),
        taskmanager_shell::FeedbackSource::Interaction
    );
    assert_eq!(
        notice.severity(),
        taskmanager_shell::FeedbackSeverity::Success
    );
    assert!(app.shell.feedback_text().contains("succeeded"));

    let _ = app.update(Message::Alerts(AlertsMessage::AddRule { rule }));
    let err_notice = app.shell.feedback_notice().expect("error notice");
    assert_eq!(
        err_notice.source(),
        taskmanager_shell::FeedbackSource::Interaction
    );
    assert_eq!(
        err_notice.severity(),
        taskmanager_shell::FeedbackSeverity::Error
    );
}

#[test]
fn remove_rule_message_applies_and_reports_notice() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));

    let _ = app.update(Message::Alerts(AlertsMessage::RemoveRule {
        rule_id: "cpu-high".into(),
    }));
    assert!(app.alerts_rules().iter().all(|m| m.rule.id != "cpu-high"));

    let notice = app.shell.feedback_notice().expect("success notice");
    assert_eq!(
        notice.source(),
        taskmanager_shell::FeedbackSource::Interaction
    );
    assert_eq!(
        notice.severity(),
        taskmanager_shell::FeedbackSeverity::Success
    );
    assert!(app.shell.feedback_text().contains("succeeded"));

    let _ = app.update(Message::Alerts(AlertsMessage::RemoveRule {
        rule_id: "missing-rule".into(),
    }));
    let warn_notice = app.shell.feedback_notice().expect("warning notice");
    assert_eq!(
        warn_notice.source(),
        taskmanager_shell::FeedbackSource::Interaction
    );
    assert_eq!(
        warn_notice.severity(),
        taskmanager_shell::FeedbackSeverity::Warning
    );
}
