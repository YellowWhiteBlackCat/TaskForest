use super::health_support::render_health_overlay;
use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use taskmanager_core::core::metrics::{MemoryScalarObservations, ScalarObservation};

use crate::demo_app;

fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    // Pin English and serialize against the language-flipping i18n test
    // (see ui::LANG_TEST_GUARD). The title/headings resolve through the
    // process-global t(), which otherwise auto-seeds from the host locale.
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_health_overlay(frame, app, crate::TuiTheme::default(), frame.area()))
        .expect("draw");
    terminal.backend().to_string()
}

#[test]
fn health_overlay_renders_domain_summary_and_alert_rules() {
    let app = demo_app();
    let text = frame_text(&app, 120, 40);
    assert!(text.contains("System health & alerts"));
    assert!(text.contains("Device status"));
    assert!(text.contains("CPU"));
    assert!(text.contains("Memory"));
    assert!(text.contains("Storage"));
    assert!(text.contains("Network"));
    assert!(text.contains("GPU"));
    assert!(text.contains("Containers"));
    assert!(text.contains("Alert rules"));
    assert!(text.contains("90.0%"));
    assert!(text.contains("Enabled"));
    assert!(text.contains("h / Esc"));
}

#[test]
fn health_overlay_renders_honest_empty_state_without_a_snapshot() {
    let mut app = demo_app();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(None)),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Hardware((None).map(Box::new)),
    );
    let text = frame_text(&app, 120, 40);
    assert!(text.contains("collecting"));
    assert!(text.contains("no provider diagnostics recorded"));
}

#[test]
fn health_overlay_marks_high_memory_usage_as_failed() {
    let mut app = demo_app();
    let mut snapshot = app.projection().snapshot.clone().expect("demo snapshot");
    snapshot.memory.apply_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(100, 1),
            used_bytes: ScalarObservation::available(99, 1),
            ..Default::default()
        },
        snapshot.memory.optional_observations().clone(),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    let text = frame_text(&app, 120, 40);
    let memory_line = text
        .lines()
        .find(|line| line.contains("Memory"))
        .expect("the memory row must render");
    assert!(memory_line.contains("failed"));
}

#[test]
fn health_overlay_keeps_a_disabled_canonical_rule_visible() {
    let mut app = TuiApp::new();
    app.shell
        .edit_alert_rules(taskmanager_application::ManagedAlertRuleEdit::Toggle {
            rule_id: "cpu-high".into(),
        })
        .unwrap();
    let text = frame_text(&app, 120, 40);
    let cpu_line = text
        .lines()
        .find(|line| line.contains("CPU usage"))
        .expect("the CPU rule row must render");
    assert!(cpu_line.contains("90.0%"));
    assert!(cpu_line.contains("Disabled"));
}

/// Parity: the diagnostics line's render input is exactly the neutral
/// VM's projection — every runtime device status keeps its historical
/// token through the kind reroute.
#[test]
fn provider_line_tokens_follow_the_neutral_kind() {
    use taskmanager_application::{SourceStateKind, device_source_line};
    use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
    use taskmanager_core::core::identity::ProviderId;

    let cases = [
        (DeviceStatus::Healthy, "ok", SourceStateKind::Ok),
        (DeviceStatus::Stale, "stale", SourceStateKind::Stale),
        (
            DeviceStatus::PermissionDenied,
            "denied",
            SourceStateKind::Failed,
        ),
        (
            DeviceStatus::MissingTool,
            "missing-tool",
            SourceStateKind::Degraded,
        ),
        (
            DeviceStatus::Unsupported,
            "unsupported",
            SourceStateKind::Unknown,
        ),
    ];
    for (status, token, kind) in cases {
        let line = device_source_line(
            &ProviderId::borrowed("demo.provider"),
            &DeviceState {
                status,
                last_success_ms: Some(1),
            },
        );
        assert_eq!(line.state, kind, "kind for {status:?}");
        assert_eq!(provider_status_label(&line), token, "token for {status:?}");
    }
}

#[test]
fn health_overlay_renders_provider_diagnostics_tokens() {
    use taskmanager_core::core::device_state::DeviceStatus;
    use taskmanager_core::core::identity::ProviderId;

    let mut app = TuiApp::new();
    let snapshot = SystemSnapshot {
        provider_states: vec![
            ProviderRuntimeState {
                provider: ProviderId::borrowed("demo.net"),
                status: DeviceStatus::Stale,
                last_success_ms: Some(4),
            },
            ProviderRuntimeState {
                provider: ProviderId::borrowed("demo.gpu"),
                status: DeviceStatus::PermissionDenied,
                last_success_ms: None,
            },
        ],
        ..SystemSnapshot::default()
    };
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
    let text = frame_text(&app, 120, 40);
    let provider_row = text
        .lines()
        .find(|line| line.contains("demo.net"))
        .expect("the provider diagnostics row must render");
    assert!(
        provider_row.contains("demo.net:stale"),
        "stale token follows the neutral fold: {provider_row}"
    );
    assert!(
        provider_row.contains("demo.gpu:denied"),
        "denied token follows the neutral fold: {provider_row}"
    );
}

#[test]
fn health_overlay_renders_cursor_on_active_rule_and_reflects_toggle() {
    let mut app = demo_app();
    app.toggle_health();
    let text = frame_text(&app, 120, 40);

    // When health is open, rule 0 is active and has the cursor marker.
    let cpu_line = text
        .lines()
        .find(|line| line.contains("CPU usage"))
        .expect("CPU rule line");
    assert!(cpu_line.contains("›"));
    assert!(cpu_line.contains("Enabled"));

    // Toggle rule 0 via toggle_selected_alert_rule.
    assert!(app.toggle_selected_alert_rule());
    let text = frame_text(&app, 120, 40);
    let cpu_line = text
        .lines()
        .find(|line| line.contains("CPU usage"))
        .expect("CPU rule line");
    assert!(cpu_line.contains("›"));
    assert!(cpu_line.contains("Disabled"));

    // Move to rule 1.
    app.health_rule_move(1);
    let text = frame_text(&app, 120, 40);
    let cpu_line = text
        .lines()
        .find(|line| line.contains("CPU usage"))
        .expect("CPU rule line");
    let mem_line = text
        .lines()
        .find(|line| line.contains("Memory usage"))
        .expect("Memory rule line");
    assert!(!cpu_line.contains("›"));
    assert!(mem_line.contains("›"));
}

#[test]
fn tui_app_alert_rule_helpers_operate_on_canonical_rules() {
    let mut app = TuiApp::new();
    assert_eq!(app.health_rule_selection(), 0);

    // Toggle by ID.
    assert!(app.toggle_alert_rule("cpu-high"));
    assert!(!app.projection().alert_center.managed_rules()[0].enabled);

    // Toggle back via edit_alert_rules.
    let outcome = app
        .edit_alert_rules(taskmanager_application::ManagedAlertRuleEdit::Toggle {
            rule_id: "cpu-high".into(),
        })
        .expect("toggle outcome");
    assert!(outcome.changed());
    assert!(app.projection().alert_center.managed_rules()[0].enabled);

    // Clamped selection move.
    app.health_rule_move(-5);
    assert_eq!(app.health_rule_selection(), 0);
    app.health_rule_move(100);
    let count = app.projection().alert_center.managed_rules().len();
    assert_eq!(app.health_rule_selection(), count - 1);
}

#[test]
fn health_overlay_renders_event_history_empty_and_populated() {
    let mut app = demo_app();
    app.toggle_health();

    // 1. Initially empty: shows title and empty state message
    let text = frame_text(&app, 120, 40);
    assert!(text.contains("Event center"), "{text}");
    assert!(text.contains("No events match this filter."), "{text}");

    // 2. Inject alert events: verify rendering
    let event = taskmanager_core::core::alerts::AlertEvent {
        id: 1,
        observed_at_ms: 5000,
        kind: taskmanager_core::core::alerts::AlertEventKind::Activated,
        alert: taskmanager_core::core::alerts::Alert {
            instance_id: "cpu-high:system-wide".into(),
            rule_id: "cpu-high".into(),
            severity: taskmanager_core::core::alerts::AlertSeverity::Critical,
            metric: taskmanager_core::core::alerts::AlertMetric::CpuUsagePercent,
            target: "system-wide".into(),
            value: 94.2,
            threshold: 90.0,
            active_since_ms: 4500,
        },
    };
    app.shell.replace_alert_event_history(vec![event]);
    let text = frame_text(&app, 120, 40);
    assert!(text.contains("[Activated]"), "{text}");
    assert!(text.contains("system-wide"), "{text}");
    assert!(text.contains("CPU usage 94.2%"), "{text}");
    assert!(text.contains("thresh: 90.0%"), "{text}");

    // 3. Clear event history via TuiApp
    app.clear_alert_event_history();
    let text = frame_text(&app, 120, 40);
    assert!(text.contains("No events match this filter."), "{text}");
}

#[test]
fn tui_app_alert_rule_authoring_and_transfer_lifecycle() {
    let mut app = TuiApp::new();

    // 1. Add a new alert rule
    let custom_rule = taskmanager_core::core::alerts::AlertRule::new(
        "custom-cpu-alert",
        taskmanager_core::core::alerts::AlertMetric::CpuUsagePercent,
        taskmanager_core::core::alerts::AlertSeverity::Warning,
        85.0,
        std::time::Duration::from_secs(5),
        5.0,
    );
    let add_outcome = app.add_alert_rule(custom_rule).expect("add rule");
    assert!(add_outcome.changed());
    assert!(
        app.projection()
            .alert_center
            .managed_rules()
            .iter()
            .any(|r| r.rule.id == "custom-cpu-alert")
    );

    // 2. Export rules to JSON
    let json = app.export_alert_rules().expect("export rules");
    assert!(json.contains("custom-cpu-alert"));

    // 3. Remove the rule
    let remove_outcome = app
        .remove_alert_rule("custom-cpu-alert".into())
        .expect("remove rule");
    assert!(remove_outcome.changed());
    assert!(
        !app.projection()
            .alert_center
            .managed_rules()
            .iter()
            .any(|r| r.rule.id == "custom-cpu-alert")
    );

    // 4. Import rules back in Replace mode
    let import_outcome = app
        .import_alert_rules(&json, taskmanager_application::AlertRuleImportMode::Replace)
        .expect("import rules");
    assert!(import_outcome.changed());
    assert!(
        app.projection()
            .alert_center
            .managed_rules()
            .iter()
            .any(|r| r.rule.id == "custom-cpu-alert")
    );

    // 5. Invalid JSON import returns an error honestly
    assert!(
        app.import_alert_rules(
            "not-json",
            taskmanager_application::AlertRuleImportMode::Merge(
                taskmanager_core::core::alerts::AlertRuleConflictPolicy::KeepExisting
            )
        )
        .is_err()
    );
}

#[test]
fn alert_event_line_formatting_covers_activated_and_cleared() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let theme = TuiTheme::default();

    // Activated with empty target defaults to metric label
    let activated = taskmanager_core::core::alerts::AlertEvent {
        id: 1,
        observed_at_ms: 1200,
        kind: taskmanager_core::core::alerts::AlertEventKind::Activated,
        alert: taskmanager_core::core::alerts::Alert {
            instance_id: "mem:".into(),
            rule_id: "mem-high".into(),
            severity: taskmanager_core::core::alerts::AlertSeverity::Warning,
            metric: taskmanager_core::core::alerts::AlertMetric::MemoryUsagePercent,
            target: "".into(),
            value: 88.0,
            threshold: 80.0,
            active_since_ms: 1000,
        },
    };
    let line1 = alert_event_line(&activated, theme);
    let s1 = line1.to_string();
    assert!(s1.contains("1200ms"), "{s1}");
    assert!(s1.contains("[Activated]"), "{s1}");
    assert!(s1.contains("Memory usage"), "{s1}");
    assert!(s1.contains("88.0%"), "{s1}");
    assert!(s1.contains("thresh: 80.0%"), "{s1}");

    // Cleared with custom target
    let cleared = taskmanager_core::core::alerts::AlertEvent {
        id: 2,
        observed_at_ms: 2400,
        kind: taskmanager_core::core::alerts::AlertEventKind::Cleared,
        alert: taskmanager_core::core::alerts::Alert {
            instance_id: "disk:/data".into(),
            rule_id: "disk-temp".into(),
            severity: taskmanager_core::core::alerts::AlertSeverity::Critical,
            metric: taskmanager_core::core::alerts::AlertMetric::DiskTemperatureC,
            target: "disk:/data".into(),
            value: 70.0,
            threshold: 90.0,
            active_since_ms: 2000,
        },
    };
    let line2 = alert_event_line(&cleared, theme);
    let s2 = line2.to_string();
    assert!(s2.contains("2400ms"), "{s2}");
    assert!(s2.contains("[Cleared]"), "{s2}");
    assert!(s2.contains("disk:/data"), "{s2}");
    assert!(s2.contains("Disk temperature 70.0°C"), "{s2}");
}
