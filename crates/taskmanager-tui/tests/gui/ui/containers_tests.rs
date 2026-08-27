use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use taskmanager_application::{FailureKind, ScalarObservation};

use crate::demo_app;

fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    // Pin English and serialize against the language-flipping i18n test
    // (see ui::LANG_TEST_GUARD). The title/headers resolve through the
    // process-global t(), which otherwise auto-seeds from the host locale.
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            render_containers_overlay(frame, app, crate::TuiTheme::default(), frame.area())
        })
        .expect("draw");
    terminal.backend().to_string()
}

#[test]
fn containers_overlay_renders_rollup_rows_and_typed_state() {
    let app = demo_app();
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("Containers"));
    assert!(text.contains("postgres"));
    assert!(text.contains("docker"));
    assert!(text.contains("12.5%"));
    assert!(text.contains("68.5 MiB"));
    assert!(text.contains("healthy · 2 container(s)"));
    assert!(text.contains("c / Esc"));
}

#[test]
fn containers_overlay_renders_typed_unavailable_state_honestly() {
    let mut app = demo_app();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Containers(Some(
            ContainerRollup::unavailable(taskmanager_application::DeviceState::default()),
        )),
    );
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("unsupported"));
    assert!(text.contains("No containers are listed"));
}

#[test]
fn containers_overlay_renders_healthy_empty_state() {
    let mut app = demo_app();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Containers(Some(
            ContainerRollup::empty_healthy(1_000),
        )),
    );
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("No containers running on this host."));
}

#[test]
fn containers_overlay_renders_unavailable_fields_as_dashes() {
    let mut app = demo_app();
    let mut containers = app.shell.projection().containers.clone();
    if let Some(rollup) = containers.as_mut()
        && let Some(container) = rollup.containers.first_mut()
    {
        container.cpu_percentage = ScalarObservation::unavailable(FailureKind::PermissionDenied);
        container.memory_bytes = ScalarObservation::unavailable(FailureKind::PermissionDenied);
    }
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Containers(containers),
    );
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("—"));
    assert!(!text.contains("0.0%"));
}

#[test]
fn containers_overlay_caps_rows_and_reports_hidden_count() {
    let (shown, hidden) = container_row_window(203);
    assert_eq!(shown, taskmanager_application::MAX_CONTAINER_ROWS);
    assert_eq!(hidden, 3);
    let label = more_rows_label(hidden);
    assert!(label.contains('3'));
    assert!(!label.contains("{count}"));
}
