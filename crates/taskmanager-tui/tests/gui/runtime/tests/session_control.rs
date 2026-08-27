//! Users-page session action menu, the shared confirmation, and the full
//! session-control round-trip through the submission queue.

use super::super::*;

use taskmanager_application::AppAction;

#[test]
fn enter_on_users_opens_the_action_menu_and_esc_closes_it() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Users));
    assert!(app.session_menu().is_none());
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.session_menu().is_some(),
        "Enter must open the Users action menu"
    );
    let session = app
        .projection()
        .sessions
        .as_ref()
        .and_then(|sessions| sessions.first())
        .expect("demo sessions");
    assert_eq!(
        app.session_menu().map(|menu| menu.session.id.as_str()),
        Some(session.id.as_str()),
        "the menu freezes the selected session"
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Esc, KeyModifiers::NONE),
    );
    assert!(app.session_menu().is_none());
}

#[test]
fn session_menu_select_opens_confirmation_and_y_confirms_disconnect() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Users));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    // Disconnect is the first entry, so Enter picks it directly.
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.shell.pending_session().is_some(),
        "picking an action must arm the shared confirmation gate"
    );
    assert!(app.session_menu().is_none(), "the menu closes on pick");
    assert_eq!(
        app.shell.pending_session().map(|pending| pending.action),
        Some(taskmanager_application::SessionControlAction::Disconnect)
    );

    // y confirms: the platform request is produced with the frozen target.
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('y'),
            KeyModifiers::NONE,
        ),
    );
    let Some(PlatformEffect::SessionControl(target)) = effect else {
        panic!("confirm must produce a SessionControl effect");
    };
    assert_eq!(
        target.action,
        taskmanager_application::SessionControlAction::Disconnect
    );
    // The first demo session has id "2".
    assert_eq!(target.session_id.as_str(), "2");
    assert!(app.shell.pending_session().is_none());
}

#[test]
fn session_confirmation_n_dismisses_without_a_platform_effect() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Users));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(app.shell.pending_session().is_some());
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('n'),
            KeyModifiers::NONE,
        ),
    );
    assert!(effect.is_none(), "dismissal must not produce an effect");
    assert!(app.shell.pending_session().is_none());
}

#[test]
fn session_control_round_trip_submits_through_queue_effect() {
    use std::sync::{Arc, Mutex};
    use taskmanager_application::{
        CapabilityCatalog, CapabilitySnapshot, EnvironmentFacets, EventEnvelope, EventPort,
        EventPortError, PlatformEvent, PlatformFacets, PlatformHandle, RequestEnvelope,
        RequestPort, SessionControlRequest, SubmissionError,
    };

    #[derive(Default)]
    struct EmptyCapabilities;
    impl CapabilityCatalog for EmptyCapabilities {
        fn snapshot(&self) -> CapabilitySnapshot {
            CapabilitySnapshot::default()
        }
    }

    #[derive(Default)]
    struct EmptyEvents;
    impl EventPort for EmptyEvents {
        type Event = PlatformEvent;

        fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
            Ok(None)
        }
    }

    #[derive(Default)]
    struct RecordingSessionControl(Mutex<Vec<SessionControlRequest>>);
    impl RequestPort for RecordingSessionControl {
        type Request = SessionControlRequest;

        fn try_submit(
            &self,
            request: RequestEnvelope<Self::Request>,
        ) -> Result<(), SubmissionError> {
            self.0
                .lock()
                .expect("recorded requests")
                .push(request.payload);
            Ok(())
        }
    }

    let recorded = Arc::new(RecordingSessionControl::default());
    let mut client = PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default()
            .with_environment(EnvironmentFacets::default().with_session_control(recorded.clone())),
    ));

    // select (menu) -> pick Disconnect (confirmation) -> y (effect) -> queue.
    let mut app = crate::demo_app();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Users));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert!(
        app.shell.pending_session().is_some(),
        "the picked action must arm the shared confirmation gate"
    );
    let effect = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('y'),
            KeyModifiers::NONE,
        ),
    )
    .expect("confirm must produce an effect");

    taskmanager_shell::queue_effect(&mut app, &mut client, effect);

    let submitted = recorded.0.lock().expect("recorded requests");
    assert_eq!(submitted.len(), 1, "exactly one request is submitted");
    let request = &submitted[0];
    assert_eq!(
        request.action,
        taskmanager_application::SessionControlAction::Disconnect,
        "the menu's Disconnect pick must reach the provider"
    );
    assert_eq!(
        request.session_id.as_str(),
        "2",
        "the provider-issued session target must be submitted"
    );
    assert_ne!(
        request.request_id.get(),
        0,
        "the latest-wins correlation id must be allocated"
    );
}

/// An active sort reorders the rendered rows; the Users action menu must
/// resolve the SAME sorted projection the renderer paints, never the
/// provider-order vector (same contract as the Services page).
#[test]
fn menu_targets_the_sorted_session_row() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    // Provider order [zeta (7), alpha (3)]; the Name sort (logon user)
    // renders [alpha (3), zeta (7)].
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sessions(Some(vec![
            taskmanager_application::SessionItem {
                id: "7".into(),
                uid: 1000,
                user: "zeta".into(),
                seat: None,
                tty: None,
                remote: false,
                timestamp: None,
            },
            taskmanager_application::SessionItem {
                id: "3".into(),
                uid: 1000,
                user: "alpha".into(),
                seat: None,
                tty: None,
                remote: false,
                timestamp: None,
            },
        ])),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Users));
    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            KeyModifiers::NONE,
        ),
    );
    // Fixture guard: the sort must actually differ from the provider order.
    assert_eq!(
        app.projection()
            .sessions
            .as_ref()
            .and_then(|sessions| sessions.first())
            .map(|session| session.id.as_str()),
        Some("7")
    );

    let _ = handle_key(
        &mut app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Enter,
            KeyModifiers::NONE,
        ),
    );
    assert_eq!(
        app.session_menu().map(|menu| menu.session.id.as_str()),
        Some("3"),
        "the menu must freeze the sorted (rendered) row, not the provider row"
    );
}
