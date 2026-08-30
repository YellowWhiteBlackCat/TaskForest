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
