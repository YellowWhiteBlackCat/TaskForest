use super::about_support::render_about_overlay;
use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::{TuiApp, TuiSurfaceKind, TuiTheme, demo_app};
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_core::core::history::HistoryWindow;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

fn frame_text(app: &TuiApp, width: u16, height: u16) -> String {
    // Pin English and serialize against the language-flipping i18n test
    // (see ui::LANG_TEST_GUARD). The title/labels resolve through the
    // process-global t(), which otherwise auto-seeds from the host locale.
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_about_overlay(frame, app, TuiTheme::default(), frame.area()))
        .expect("draw");
    terminal.backend().to_string()
}

#[test]
fn about_overlay_renders_hardware_facts_and_version() {
    let app = demo_app();
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("About TaskForest"));
    assert!(text.contains(VERSION));
    assert!(text.contains("taskforest-workstation"));
    assert!(text.contains("Arch Linux"));
    assert!(text.contains("6.18.7-arch1-1"));
    // The fixture ships the provider-verbatim brand string (Intel reports the
    // trademark markers); the About overlay paints the fact verbatim, exactly
    // like the System page — no normalization layer exists or is claimed.
    assert!(text.contains("Intel(R) Core(TM) Ultra 7 358H"));
    assert!(text.contains("22"));
    assert!(text.contains("32.0 GiB"));
    assert!(text.contains("06h 42m"));
    assert!(text.contains("i / Esc"));
}

#[test]
fn about_overlay_renders_dashes_when_telemetry_is_missing() {
    let mut app = demo_app();
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Hardware((None).map(Box::new)),
    );
    seed_projection_fact(&mut app.shell, ProjectionSeedFact::Snapshot(Box::new(None)));
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("Hostname"));
    assert!(text.contains('—'));
}

#[test]
fn capture_scene_overrides_activate_expected_state() {
    use crate::demo::apply_capture_scene_override;
    use taskmanager_application::{AppPage, ConfirmationKind};

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "process-force-kill");
    assert_eq!(
        app.shell.application.interaction.confirmation_kind(),
        Some(ConfirmationKind::ProcessBatch)
    );

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "smart-self-test-confirm");
    assert!(app.shell.pending_smart_self_test().is_some());

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "service-details-logs");
    assert!(app.shell.service_log.is_some());

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "about");
    assert!(app.about_open());

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "storage-health");
    assert!(app.health_open());

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "telemetry-paused");
    assert!(app.shell.paused());

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "diagnostic-preview");
    assert_eq!(
        app.local_surface_kind(),
        Some(TuiSurfaceKind::DiagnosticPreview)
    );

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "diagnostic-failure");
    assert_eq!(
        app.local_surface_kind(),
        Some(TuiSurfaceKind::DiagnosticFailure)
    );

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "history-replay");
    assert!(app.history_replay_open());
    assert_eq!(app.shell.page(), AppPage::Performance);

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "history-60m");
    assert!(app.history_replay_open());
    assert_eq!(app.history_replay_window(), HistoryWindow::OneHour);
    assert_eq!(app.shell.page(), AppPage::Performance);

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "event-center");
    assert_eq!(app.local_surface_kind(), Some(TuiSurfaceKind::Health));
    assert!(
        !app.shell
            .projection()
            .alert_center
            .event_history()
            .is_empty()
    );

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "settings-permission-center");
    assert_eq!(app.local_surface_kind(), Some(TuiSurfaceKind::Settings));

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "apps-identity-matrix");
    assert_eq!(app.shell.page(), AppPage::Applications);
    assert!(
        app.shell
            .projection()
            .processes
            .as_ref()
            .is_some_and(|p| p.iter().any(|proc| proc.cmdline.contains("chrome")))
    );
}
