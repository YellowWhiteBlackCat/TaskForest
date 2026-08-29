use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::demo_app;

fn frame_text(app: &crate::TuiApp, width: u16, height: u16) -> String {
    // Pin English and serialize against the language-flipping i18n test
    // (see ui::LANG_TEST_GUARD). The title/labels resolve through the
    // process-global t(), which otherwise auto-seeds from the host locale.
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_about_overlay(frame, app, crate::TuiTheme::default(), frame.area()))
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
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Hardware((None).map(Box::new)),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(None)),
    );
    let text = frame_text(&app, 120, 36);
    assert!(text.contains("Hostname"));
    assert!(text.contains('—'));
}
