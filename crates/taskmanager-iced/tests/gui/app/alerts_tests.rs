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
    // Both stops are part of the frozen registry the uniqueness gate
    // walks (FocusTarget::ALL).
    assert!(FocusTarget::ALL.contains(&FocusTarget::AlertsPageTab));
    assert!(FocusTarget::ALL.contains(&FocusTarget::AlertsRuleToggle(0)));
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
