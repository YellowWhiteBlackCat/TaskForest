use super::about_support::render_about_overlay;
use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::{TuiApp, TuiSurfaceKind, TuiTheme, demo_app};
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_core::core::history::HistoryWindow;

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
fn about_reviews_application_metadata_and_keeps_system_information_separate() {
    let mut app = demo_app();
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("About TaskForest"));
    assert!(text.contains(VERSION));
    assert!(text.contains("Apache-2.0"));
    app.toggle_about();
    assert!(
        app.information_copy_payload()
            .expect("About details")
            .contains("github.com/YellowWhiteBlackCat/TaskForest")
    );
    app.open_system_information();
    assert_eq!(
        app.local_surface_kind(),
        Some(TuiSurfaceKind::SystemInformation)
    );
    let text = app.information_copy_payload().expect("system information");
    assert!(text.contains("taskforest-workstation"));
    assert!(text.contains("Intel(R) Core(TM) Ultra 7 358H"));
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
        Some(TuiSurfaceKind::DiagnosticBundle)
    );

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "diagnostic-failure");
    assert_eq!(
        app.local_surface_kind(),
        Some(TuiSurfaceKind::DiagnosticBundle)
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
    apply_capture_scene_override(&mut app, "first-run");
    assert_eq!(app.local_surface_kind(), Some(TuiSurfaceKind::FirstRun));

    let mut app = demo_app();
    apply_capture_scene_override(&mut app, "saved-view-presets");
    assert_eq!(app.local_surface_kind(), Some(TuiSurfaceKind::ColumnMenu));

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

fn review_frame(app: &TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang guard");
    set_language(Language::En);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| crate::ui::render(frame, app, TuiTheme::default()))
        .expect("draw review");
    terminal.backend().to_string()
}

fn review_popup_text(app: &TuiApp, width: u16, height: u16) -> String {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang guard");
    set_language(Language::En);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| crate::ui::render(frame, app, TuiTheme::default()))
        .expect("draw review");
    let kind = app.local_surface_kind().expect("review surface");
    let popup = crate::ui::frame_plan::overlay_popup(
        ratatui::layout::Rect::new(0, 0, width, height),
        crate::TuiInputScope::LocalSurface(kind),
    )
    .expect("popup");
    let inner = popup.inner(ratatui::layout::Margin::new(1, 1));
    (inner.y..inner.bottom())
        .map(|y| {
            (inner.x..inner.right())
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<String>()
}

#[test]
fn compact_information_reviews_keep_actions_visible_and_complete_values_reachable() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use taskmanager_shell::fixture::edit_hardware;
    let mut app = demo_app();
    app.toggle_about();
    for (width, height) in [(54, 16), (120, 36), (160, 20), (72, 60)] {
        let text = review_frame(&app, width, height);
        for action in [
            "Open repository",
            "System information",
            "Copy details",
            "Close",
            "Up/Down",
        ] {
            assert!(
                text.contains(action),
                "{width}x{height} must keep {action} reachable: {text}"
            );
        }
    }
    let _ =
        crate::information::handle_key(&mut app, KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    let text = review_popup_text(&app, 54, 16);
    assert!(
        text.contains("YellowWhiteBlackCat/TaskForest"),
        "complete repository remains reachable: {text}"
    );
    edit_hardware(&mut app.shell, |hardware| {
        hardware.as_mut().expect("fixture hardware").cpu_brand = Some("Observed CPU ".repeat(30));
    });
    app.open_system_information();
    let frozen = app.information_copy_payload().expect("frozen facts");
    edit_hardware(&mut app.shell, |hardware| {
        hardware.as_mut().expect("fixture hardware").cpu_brand = Some("New observation".to_owned());
    });
    assert_eq!(
        app.information_copy_payload().as_deref(),
        Some(frozen.as_str())
    );
    let _ =
        crate::information::handle_key(&mut app, KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    let text = review_frame(&app, 54, 16);
    for action in ["Copy all", "Close", "Scroll for more details"] {
        assert!(text.contains(action), "{action} fixed in frame: {text}");
    }
    assert!(
        text.contains("Memory: 32.0 GiB"),
        "last fact must be visible after End: {text}"
    );
    let mut copied = Vec::new();
    app.copy_information_to(&mut copied);
    assert_eq!(
        copied,
        format!(
            "\x1b]52;c;{}\x07",
            crate::clipboard::base64_encode(frozen.as_bytes())
        )
        .as_bytes()
    );
}
