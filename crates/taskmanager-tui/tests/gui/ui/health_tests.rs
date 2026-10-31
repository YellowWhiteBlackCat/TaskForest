use super::health_support::render_health_overlay;
use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use taskforest_core::core::metrics::{MemoryScalarObservations, ScalarObservation};

use crate::demo_app;
use taskforest_core::core::alerts::{
    Alert, AlertEventKind, AlertInstanceId, AlertMetric, AlertRuleId, AlertSeverity,
    AlertTargetIdentity,
};
use taskforest_core::core::identity::{DeviceGeneration, DeviceId};
use taskmanager_application::AlertEventSeed;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_shell::fixture::health::seed_shell_health;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

use crate::health_review::HealthReviewMode;

fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    frame_text_in_language(app, width, height, Language::En)
}

fn frame_text_in_language(app: &TuiApp, width: u16, height: u16, language: Language) -> String {
    // Pin English and serialize against the language-flipping i18n test
    // (see ui::LANG_TEST_GUARD). The title/headings resolve through the
    // process-global t(), which otherwise auto-seeds from the host locale.
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(language);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_health_overlay(frame, app, crate::TuiTheme::default(), frame.area()))
        .expect("draw");
    terminal.backend().to_string()
}

#[test]
fn health_review_footer_paints_localized_protocol_hints_in_compact_frames() {
    let mut app = demo_app();
    seed_shell_health(&mut app.shell);
    app.toggle_health();
    app.select_health_review(HealthReviewMode::Storage);

    for (language, expected) in [
        (
            Language::En,
            [
                "Up/Down groups · p Alerts · e Event center",
                "w Storage health · s Sensors · q Alert rules",
            ],
        ),
        (
            Language::Zh,
            [
                "Up/Down groups · p 告警 · e 通知与事件中心",
                "w 存储健康 · s 传感器 · q 告警规则",
            ],
        ),
    ] {
        for (width, height) in [(54, 16), (80, 24)] {
            let text = frame_text_in_language(&app, width, height, language);
            for line in expected {
                assert!(
                    text.lines().any(|painted| painted.contains(line)),
                    "{line:?} must be painted in {width}x{height} under {language:?}:\n{text}"
                );
            }
        }
    }
}

#[test]
fn health_review_events_footer_keeps_all_mode_and_event_actions_at_compact_sizes() {
    let mut app = demo_app();
    app.toggle_health();
    app.select_health_review(HealthReviewMode::Events);

    for (width, height) in [(54, 16), (80, 24)] {
        let text = frame_text(&app, width, height);
        for expected in [
            "Up/Down groups · p Alerts · e Event center",
            "w Storage health · s Sensors · q Alert rules",
            "h / Esc Close",
        ] {
            assert!(
                text.lines().any(|line| line.contains(expected)),
                "{expected:?} must be painted in Events mode at {width}x{height}:\n{text}"
            );
        }
        let event_actions = text
            .lines()
            .find(|line| line.contains("F2 Filter"))
            .expect("Events filter/export/clear row");
        assert!(event_actions.contains("F5 Export"), "{text}");
        assert!(event_actions.contains("F8 Clear"), "{text}");
    }
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
fn compact_rule_review_keeps_selection_and_toggle_controls_reachable() {
    let mut app = demo_app();
    app.toggle_health();
    app.health_rule_selection = app.projection().alert_center.managed_rules().len() - 1;
    for (width, height) in [(54, 16), (80, 24), (120, 40), (160, 16), (54, 48)] {
        let text = frame_text(&app, width, height);
        assert!(text.contains("Threshold:"), "threshold at {width}x{height}");
        assert!(text.contains("Duration:"));
        assert!(text.contains("Hysteresis:"));
        assert!(text.contains("Target:"));
        assert!(
            text.contains("h / Esc Close"),
            "close action is inside the frame"
        );
        assert!(text.contains("Enter/Space Toggle"));
        assert!(!text.contains("n Add"));
        assert!(!text.contains("y Export"));
        assert!(!text.contains("a Merge"));
        assert!(!text.contains("r Replace"));
        assert!(!text.contains("z Short test"));
        assert!(!text.contains("x Extended test"));
    }
}

#[test]
fn every_health_group_and_fixed_action_fits_the_minimum_terminal() {
    set_language(Language::En);
    let mut app = demo_app();
    seed_shell_health(&mut app.shell);
    app.toggle_health();
    for mode in [HealthReviewMode::Storage, HealthReviewMode::Sensors] {
        app.select_health_review(mode);
        let groups = crate::ui::health_review::groups(&app);
        for (index, group) in groups.iter().enumerate() {
            app.health_review.selected = index;
            let text = frame_text(&app, 54, 16);
            assert!(text.contains("h / Esc Close"));
            assert!(!text.contains("z Short test"));
            assert!(!text.contains("x Extended test"));
            for row in &group.rows {
                assert!(
                    text.contains(&row.label),
                    "{} is reachable in {:?}",
                    row.label,
                    mode
                );
            }
        }
    }
}

#[test]
fn health_overlay_renders_honest_empty_state_without_a_snapshot() {
    let mut app = demo_app();
    seed_projection_fact(&mut app.shell, ProjectionSeedFact::Snapshot(Box::new(None)));
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Hardware((None).map(Box::new)),
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
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
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
    assert!(app.toggle_alert_rule("cpu-high"));
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
    use taskforest_core::core::device_state::{DeviceState, DeviceStatus};
    use taskforest_core::core::identity::ProviderId;
    use taskmanager_application::{SourceStateKind, device_source_line};

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
    use taskforest_core::core::device_state::DeviceStatus;
    use taskforest_core::core::identity::ProviderId;

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
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
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

    // Toggle back through the supported TUI rule-toggle path.
    assert!(app.toggle_alert_rule("cpu-high"));
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
    let cpu_rule_id = AlertRuleId::new("cpu-high");
    app.shell.replace_alert_event_history(vec![AlertEventSeed {
        observed_at_ms: 5000,
        kind: AlertEventKind::Activated,
        alert: Alert {
            instance_id: AlertInstanceId::new(cpu_rule_id.clone(), AlertTargetIdentity::System),
            rule_id: cpu_rule_id,
            severity: AlertSeverity::Critical,
            metric: AlertMetric::CpuUsagePercent,
            target: "system-wide".into(),
            value: 94.2,
            threshold: 90.0,
            active_since_ms: 4500,
        },
    }]);
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
fn alert_event_line_formatting_covers_activated_and_cleared() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let theme = TuiTheme::default();

    // Activated with empty target defaults to metric label
    let mut app = TuiApp::new();
    let memory_rule_id = AlertRuleId::new("mem-high");
    let disk_rule_id = AlertRuleId::new("disk-temp");
    app.shell.replace_alert_event_history(vec![
        AlertEventSeed {
            observed_at_ms: 1200,
            kind: AlertEventKind::Activated,
            alert: Alert {
                instance_id: AlertInstanceId::new(
                    memory_rule_id.clone(),
                    AlertTargetIdentity::System,
                ),
                rule_id: memory_rule_id,
                severity: AlertSeverity::Warning,
                metric: AlertMetric::MemoryUsagePercent,
                target: "".into(),
                value: 88.0,
                threshold: 80.0,
                active_since_ms: 1000,
            },
        },
        AlertEventSeed {
            observed_at_ms: 2400,
            kind: AlertEventKind::Cleared,
            alert: Alert {
                instance_id: AlertInstanceId::new(
                    disk_rule_id.clone(),
                    AlertTargetIdentity::Storage {
                        device_id: DeviceId::new("disk:/data"),
                        generation: DeviceGeneration::new(1),
                    },
                ),
                rule_id: disk_rule_id,
                severity: AlertSeverity::Critical,
                metric: AlertMetric::DiskTemperatureC,
                target: "disk:/data".into(),
                value: 70.0,
                threshold: 90.0,
                active_since_ms: 2000,
            },
        },
    ]);
    let events = app.projection().alert_center.event_history();
    let activated = events.first().expect("activated event");
    let line1 = alert_event_line(activated, theme);
    let s1 = line1.to_string();
    assert!(s1.contains("1200ms"), "{s1}");
    assert!(s1.contains("[Activated]"), "{s1}");
    assert!(s1.contains("Memory usage"), "{s1}");
    assert!(s1.contains("88.0%"), "{s1}");
    assert!(s1.contains("thresh: 80.0%"), "{s1}");

    // Cleared with custom target
    let cleared = events.get(1).expect("cleared event");
    let line2 = alert_event_line(cleared, theme);
    let s2 = line2.to_string();
    assert!(s2.contains("2400ms"), "{s2}");
    assert!(s2.contains("[Cleared]"), "{s2}");
    assert!(s2.contains("disk:/data"), "{s2}");
    assert!(s2.contains("Disk temperature 70.0°C"), "{s2}");
}
