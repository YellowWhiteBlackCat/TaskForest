//! test-intent: behavior

use super::*;
use taskmanager_application::first_run::FirstRunController;
use taskmanager_application::{
    CorrelatedSetupScriptEvent, PlatformEventBatch, PlatformEventContext,
};
use taskmanager_core::core::setup::SetupScriptEvent;
use taskmanager_platform_contract::{CapabilityId, EventSequence};
use taskmanager_shell::fixture::setup::setup_script_info;
use taskmanager_test_support::setup_script::platform;

#[test]
fn settings_review_is_explicit_and_commands_use_the_native_typed_port_once() {
    let (mut platform, recorder) = platform();
    let mut app = crate::demo_app();
    app.first_run.observe(Some(&mut platform), 1);
    let request = recorder.submissions().expect("recorded")[0].id;
    let event = CorrelatedSetupScriptEvent::new(
        PlatformEventContext {
            request_id: request,
            capability: CapabilityId::FIRST_RUN_SETUP,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
        },
        SetupScriptEvent::Observed(Some(setup_script_info())),
    );
    app.apply_platform_batch(PlatformEventBatch {
        setup_script_events: vec![event],
        ..Default::default()
    });
    assert!(app.local_surface().is_none());
    app.open_local_surface(TuiSurface::Settings);
    let _ = crate::runtime::handle_settings_key(
        &mut app,
        KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE),
    );
    assert_eq!(
        app.local_surface_kind(),
        Some(crate::TuiSurfaceKind::FirstRun)
    );
    assert_eq!(
        app.first_run_copy_payload('2'),
        Some(setup_script_info().run_command)
    );
    let effect = handle_key(
        &mut app,
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
    )
    .expect("explicit Run");
    let PlatformEffect::SetupScript(request) = effect else {
        panic!("typed action")
    };
    app.submit_first_run_action(request.action, Some(&mut platform), 2);
    assert_eq!(app.first_run.view().phase, FirstRunPhase::Running);
    assert!(
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)
        )
        .is_none()
    );
    assert_eq!(recorder.submissions().expect("recorded").len(), 2);
    handle_key(&mut app, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.local_surface().is_none());
    app.submit_first_run_action(SetupScriptAction::Revert, Some(&mut platform), 3);
    assert_eq!(
        recorder.submissions().expect("recorded").len(),
        2,
        "a stale intent stays inert after dismissal"
    );
}

#[test]
fn unavailable_setup_has_no_entry_and_compact_review_keeps_all_actions() {
    use ratatui::{Terminal, backend::TestBackend};
    use taskmanager_application::i18n::{Language, set_language};
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("language guard");
    set_language(Language::En);
    let mut app = crate::demo_app();
    app.open_first_run();
    assert!(app.local_surface().is_none());
    app.first_run = FirstRunController::from_observation(Some(setup_script_info()));
    app.open_first_run();
    let mut terminal = Terminal::new(TestBackend::new(54, 16)).expect("terminal");
    terminal
        .draw(|frame| crate::ui::render(frame, &app, crate::TuiTheme::default()))
        .expect("render");
    let text = terminal.backend().to_string();
    assert!(text.contains(t("first_run.run_setup")), "{text}");
    assert!(text.contains(t("first_run.revert_setup")));
    assert!(text.contains("Esc Close"));
    assert!(text.contains("Up/Down/PgUp/PgDn"));
    handle_key(&mut app, KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    terminal
        .draw(|frame| crate::ui::render(frame, &app, crate::TuiTheme::default()))
        .expect("scroll to final metadata");
    let scrolled = terminal.backend().to_string();
    assert!(
        scrolled.contains(t("first_run.open_docs")),
        "the complete descriptor and docs hints remain reachable: {scrolled}"
    );
    assert!(scrolled.contains("Esc Close"));
    let mut normal = Terminal::new(TestBackend::new(120, 36)).expect("normal terminal");
    handle_key(&mut app, KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    normal
        .draw(|frame| crate::ui::render(frame, &app, crate::TuiTheme::default()))
        .expect("normal review");
    let text = normal.backend().to_string();
    assert!(text.contains(&setup_script_info().run_command));
    assert!(text.contains(&setup_script_info().revert_command));
    assert!(text.contains(t("first_run.open_docs")));
}
