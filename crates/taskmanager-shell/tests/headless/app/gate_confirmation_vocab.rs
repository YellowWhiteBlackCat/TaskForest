//! The shared gate-confirmation vocabulary (2026-08-17 uplift): every armed
//! destructive-action gate — service control, process batch, session
//! control, startup control — owns 'y'/'n' directly in
//! [`ShellApp::handle_local_char`], so frontends route armed-gate keys
//! through the shared state machine instead of carrying per-frontend copies
//! (the TUI's four local blocks were collapsed into this vocabulary).

use super::super::*;
use taskmanager_application::AppAction;
use taskmanager_core::core::process::ProcessBatchAction;
use taskmanager_core::core::services::ServiceAction;
use taskmanager_core::core::session::SessionControlAction;

#[test]
fn armed_service_gate_confirms_and_dismisses_through_handle_local_char() {
    let mut app = crate::demo_app();
    let service = app.data.services.as_ref().expect("demo services")[0].clone();
    assert!(app.select_service_control(&service, ServiceAction::Stop));
    assert_eq!(app.apply_action(AppAction::RequestServiceControl), None);
    assert!(app.pending_service_control().is_some());

    // The armed gate owns the keyboard: 'q' must NOT quit while armed.
    app.handle_local_char('q', Modifiers::NONE);
    assert!(!app.should_quit(), "an armed gate swallows plain bindings");

    // 'n' dismisses without submitting.
    assert_eq!(
        app.handle_local_char('n', Modifiers::NONE),
        InputDispatch::Consumed
    );
    assert!(app.pending_service_control().is_none());

    // 'y' emits the frozen request through the shared confirm action.
    assert!(app.select_service_control(&service, ServiceAction::Restart));
    let _ = app.apply_action(AppAction::RequestServiceControl);
    let confirmed = app.handle_local_char('y', Modifiers::NONE);
    assert!(matches!(
        confirmed,
        InputDispatch::Effect(ref effect)
            if matches!(effect.as_ref(), PlatformEffect::ServiceControl(target)
                if target.service_id == service.id && target.action == ServiceAction::Restart)
    ));
    assert!(app.pending_service_control().is_none());
}

#[test]
fn armed_batch_gate_confirms_and_dismisses_through_handle_local_char() {
    let mut app = crate::demo_app();
    // A destructive Kill over the selected row arms the gate.
    assert_eq!(app.request_process_batch(ProcessBatchAction::Kill), None);
    assert!(app.pending_batch().is_some());

    app.handle_local_char('q', Modifiers::NONE);
    assert!(!app.should_quit(), "an armed gate swallows plain bindings");
    assert_eq!(
        app.handle_local_char('n', Modifiers::NONE),
        InputDispatch::Consumed
    );
    assert!(app.pending_batch().is_none());

    assert_eq!(app.request_process_batch(ProcessBatchAction::Kill), None);
    let confirmed = app.handle_local_char('y', Modifiers::NONE);
    assert!(
        matches!(confirmed, InputDispatch::Effect(effect)
            if matches!(effect.as_ref(), PlatformEffect::ExecuteBatch(_))),
        "the batch gate must emit the frozen ExecuteBatch effect on 'y'"
    );
    assert!(app.pending_batch().is_none());
}

#[test]
fn armed_session_gate_confirms_and_dismisses_through_handle_local_char() {
    let mut app = crate::demo_app();
    let session = app.data.sessions.as_ref().expect("demo sessions")[0].clone();
    assert!(app.select_session_control(&session, SessionControlAction::Lock));
    assert!(app.pending_session().is_some());

    app.handle_local_char('q', Modifiers::NONE);
    assert!(!app.should_quit(), "an armed gate swallows plain bindings");
    assert_eq!(
        app.handle_local_char('n', Modifiers::NONE),
        InputDispatch::Consumed
    );
    assert!(app.pending_session().is_none());

    assert!(app.select_session_control(&session, SessionControlAction::Disconnect));
    let InputDispatch::Effect(effect) = app.handle_local_char('y', Modifiers::NONE) else {
        panic!("the session gate must emit the SessionControl effect on 'y'");
    };
    let PlatformEffect::SessionControl(target) = *effect else {
        panic!("the session gate emitted the wrong platform effect");
    };
    assert_eq!(target.session_id.as_str(), session.id.as_str());
    assert!(app.pending_session().is_none());
}

#[test]
fn armed_startup_gate_confirms_and_dismisses_through_handle_local_char() {
    let mut app = crate::demo_app();
    let entry = app.data.startup_entries.as_ref().expect("demo startup")[0].clone();
    assert_eq!(app.request_startup_control_for(entry.clone(), false), None);
    assert!(app.pending_startup().is_some());

    app.handle_local_char('q', Modifiers::NONE);
    assert!(!app.should_quit(), "an armed gate swallows plain bindings");
    assert_eq!(
        app.handle_local_char('n', Modifiers::NONE),
        InputDispatch::Consumed
    );
    assert!(app.pending_startup().is_none());

    assert_eq!(app.request_startup_control_for(entry.clone(), true), None);
    let confirmed = app.handle_local_char('y', Modifiers::NONE);
    assert!(
        matches!(confirmed, InputDispatch::Effect(effect)
            if matches!(effect.as_ref(), PlatformEffect::StartupControl(_))),
        "the startup gate must emit the StartupControl effect on 'y'"
    );
    assert!(app.pending_startup().is_none());
}
