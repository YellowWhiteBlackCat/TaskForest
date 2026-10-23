//! test-intent: behavior
//!
//! Headless behavior tests for the Sessions page (same two layers as the
//! services page tests): the pure row view model projects through the shared
//! sessions sort with an honest seat/tty summary (missing fields render the
//! shared marker, never a fabricated empty), the identity-keyed selection survives
//! re-sorts, the control-outcome feedback names action and target, and the
//! wired `MinimalPlugins` page renders folded rows, repaints only on
//! sessions-domain folds, routes header clicks through the shell's sort entry
//! and resolves clicked/moved rows only through the sorted shell projection.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use taskmanager_theme::tokens::UiSize;

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ui::BackgroundColor;
use taskmanager_application::i18n::t;
use taskmanager_application::{
    CorrelatedSessionEvent, EnvironmentFacets, HostTelemetryRequest, LatestControlRequest,
    PlatformClient, PlatformEvent, PlatformEventBatch, PlatformFacets, PlatformHandle,
    SessionControlOutcome, SessionControlRequest, SessionEvent, SystemFacets,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::session::{
    SessionControlAction, SessionIdentity, SessionIncarnation, SessionItem,
};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::core::target::SessionId;
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    EventEnvelope, EventPort, EventPortError, EventSequence, PartialSourceSnapshot,
    RequestEnvelope, RequestId, RequestPort, SubmissionError,
};

use taskmanager_shell::presentation::MISSING_VALUE;
use taskmanager_shell::{InfoSortCol, InfoTable, ShellApp, SortDir};
use taskmanager_theme::Theme;

use super::{
    SessionKind, SessionKindMarker, SessionRowClicked, SessionSelection, SessionSelectionMoved,
    SessionSortClicked, SessionsRowMarker, SessionsStatusLine, bubble, empty_state_text,
    feedback_line_text, kind, moved_row, selected_row, session_rows, session_seat_text,
    session_tty_text,
};
use crate::app::{FrontendTrack, Page, Route, RouteChanged};
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;

// ---- fixtures ----

fn session_item(id: &str, user: &str, seat: Option<&str>, tty: Option<&str>) -> SessionItem {
    SessionItem {
        id: id.to_owned().into(),
        incarnation: Some(SessionIncarnation::LinuxLogind {
            boot_id: "fixture-boot".into(),
            created_monotonic_usec: id.parse().unwrap_or(1),
        }),
        uid: 1000,
        user: user.to_owned(),
        seat: seat.map(str::to_owned),
        tty: tty.map(str::to_owned),
        remote: false,
        timestamp: Some("2026-08-24 09:00".to_owned()),
    }
}

fn sessions_batch(items: Vec<SessionItem>) -> PlatformEventBatch {
    PlatformEventBatch {
        session_events: vec![CorrelatedSessionEvent {
            request_id: RequestId::MIN,
            capability: CapabilityId::SESSIONS,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
            event: SessionEvent::Snapshot(PartialSourceSnapshot {
                items,
                sources: Vec::new(),
            }),
        }],
        ..PlatformEventBatch::default()
    }
}

fn failed_source() -> Vec<SourceStatus> {
    vec![SourceStatus {
        provider: ProviderId::borrowed("test.sessions"),
        outcome: SourceOutcome::Unavailable(FailureKind::TimedOut),
        item_count: 0,
    }]
}

struct FixedCapabilities(CapabilitySnapshot);

impl CapabilityCatalog for FixedCapabilities {
    fn snapshot(&self) -> CapabilitySnapshot {
        self.0.clone()
    }
}

#[derive(Default)]
struct ScriptedEvents(Mutex<VecDeque<EventEnvelope<PlatformEvent>>>);

impl EventPort for ScriptedEvents {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(self.0.lock().expect("scripted port lock").pop_front())
    }
}

struct QuietRequests;

impl RequestPort for QuietRequests {
    type Request = HostTelemetryRequest;

    fn try_submit(&self, _request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        Ok(())
    }
}

fn descriptor(id: CapabilityId, status: CapabilityStatus) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id,
        status,
        providers: Vec::new(),
        observed_at_ms: 1,
        last_success_at_ms: None,
    }
}

/// Records every session-control request the frontend submits to the
/// platform: the observable half of "the bubble confirmed the frozen target".
#[derive(Default)]
struct RecordedSessionControls(Mutex<Vec<SessionControlRequest>>);

impl RequestPort for RecordedSessionControls {
    type Request = SessionControlRequest;

    fn try_submit(&self, request: RequestEnvelope<Self::Request>) -> Result<(), SubmissionError> {
        self.0
            .lock()
            .expect("session control recorder lock")
            .push(request.payload);
        Ok(())
    }
}

fn headless_sessions_app() -> (App, Arc<ScriptedEvents>) {
    headless_sessions_app_with_controls(Arc::new(RecordedSessionControls::default()))
}

/// The same composition, with a recorder on the session-control port so a
/// test can observe the submitted request instead of the queue it passed
/// through.
fn headless_sessions_app_with_controls(
    controls: Arc<RecordedSessionControls>,
) -> (App, Arc<ScriptedEvents>) {
    let events = Arc::new(ScriptedEvents::default());
    let snapshot = CapabilitySnapshot::from_descriptors([descriptor(
        CapabilityId::SESSIONS,
        CapabilityStatus::Available,
    )]);
    let port = events.clone();
    let client = PlatformClient::new(PlatformHandle::new(
        Arc::new(FixedCapabilities(snapshot)),
        port,
        PlatformFacets::default()
            .with_system(SystemFacets::default().with_host(Arc::new(QuietRequests)))
            .with_environment(EnvironmentFacets::default().with_session_control(controls.clone())),
    ));
    let cache: &'static RuntimeCache = Box::leak(Box::new(RuntimeCache::new()));
    let runtime: &'static SharedRuntime = cache
        .get_or_init(move || Ok(client))
        .expect("scripted runtime starts");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(HeadlessFrontendPlugins);
    app.add_plugins(FrontendWindowPlugin {
        runtime,
        palette: ui_palette(&Theme::dark(), UiSize::Standard),
    });
    app.init_resource::<bevy::asset::Assets<bevy::text::Font>>();
    (app, events)
}

fn push_sessions(events: &ScriptedEvents, items: Vec<SessionItem>) {
    events
        .0
        .lock()
        .expect("scripted port lock")
        .push_back(EventEnvelope {
            request_id: RequestId::MIN,
            capability: CapabilityId::SESSIONS,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
            outcome: Ok(PlatformEvent::Sessions(SessionEvent::Snapshot(
                PartialSourceSnapshot {
                    items,
                    sources: Vec::new(),
                },
            ))),
        });
}

/// Route before the first update: the app mounts the Sessions page on frame 1
/// and never mounts the Processes default route (out of this page's scope).
fn route_to_sessions(app: &mut App) {
    app.world_mut().resource_mut::<Route>().page = Page::Sessions;
    app.world_mut().commands().trigger(RouteChanged);
}

fn row_targets(app: &mut App) -> Vec<(usize, String)> {
    app.world_mut()
        .query_filtered::<&SessionsRowMarker, ()>()
        .iter(app.world())
        .map(|marker| (marker.0, marker.1.to_string()))
        .collect()
}

fn row_entities(app: &mut App) -> Vec<(Entity, String)> {
    app.world_mut()
        .query_filtered::<(Entity, &SessionsRowMarker), ()>()
        .iter(app.world())
        .map(|(entity, marker)| (entity, marker.1.to_string()))
        .collect()
}

fn status_line(app: &mut App) -> String {
    app.world_mut()
        .query_filtered::<&bevy::ui::widget::Text, With<SessionsStatusLine>>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .next()
        .unwrap_or_default()
}

fn selected_row_target(app: &mut App) -> Option<String> {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let highlight = palette.nav_active_bg.to_srgba();
    app.world_mut()
        .query_filtered::<(&SessionsRowMarker, &BackgroundColor), ()>()
        .iter(app.world())
        .find(|(_, fill)| fill.0.to_srgba() == highlight)
        .and_then(|(marker, _)| marker.2.as_ref())
        .map(|identity| identity.session_id.to_string())
}

// ---- pure: row model, seat/tty summary, feedback, selection ----

#[test]
fn seat_and_tty_summaries_render_the_shared_missing_marker() {
    let observed = session_item("2", "root", Some("seat0"), Some("tty2"));
    assert_eq!(session_seat_text(&observed), "seat0");
    assert_eq!(session_tty_text(&observed), "tty2");
    let graphical = session_item("3", "ada", None, None);
    assert_eq!(
        session_seat_text(&graphical),
        MISSING_VALUE,
        "an unobserved seat renders the shared marker, never an empty string"
    );
    assert_eq!(session_tty_text(&graphical), MISSING_VALUE);
}

#[test]
fn rows_project_through_the_shared_sessions_sort() {
    let mut shell = ShellApp::new();
    shell.apply_platform_batch(sessions_batch(vec![
        session_item("5", "zoe", Some("seat0"), None),
        session_item("2", "ada", Some("seat0"), Some("tty2")),
        session_item("9", "miguel", None, None),
    ]));
    let provider_order: Vec<String> = session_rows(&shell)
        .into_iter()
        .map(|row| row.id.to_string())
        .collect();
    assert_eq!(provider_order, ["5", "2", "9"]);
    shell.set_info_sort(InfoTable::Users, InfoSortCol::Name);
    let by_user: Vec<String> = session_rows(&shell)
        .into_iter()
        .map(|row| row.user)
        .collect();
    assert_eq!(by_user, ["ada", "miguel", "zoe"]);
    shell.set_info_sort(InfoTable::Users, InfoSortCol::Session);
    let by_session: Vec<String> = session_rows(&shell)
        .into_iter()
        .map(|row| row.session)
        .collect();
    assert_eq!(by_session, ["2", "5", "9"]);
    shell.set_info_sort(InfoTable::Users, InfoSortCol::Seat);
    let by_seat: Vec<String> = session_rows(&shell)
        .into_iter()
        .map(|row| row.seat)
        .collect();
    assert_eq!(
        by_seat,
        [
            MISSING_VALUE.to_owned(),
            "seat0".to_owned(),
            "seat0".to_owned()
        ],
        "seat sort groups the observed seats before the unobserved marker"
    );
}

#[test]
fn selection_is_id_keyed_and_survives_a_sort_flip() {
    let mut shell = ShellApp::new();
    shell.apply_platform_batch(sessions_batch(vec![
        session_item("5", "zoe", None, None),
        session_item("2", "ada", None, None),
    ]));
    let identity = SessionIdentity {
        session_id: SessionId::new("2"),
        incarnation: SessionIncarnation::LinuxLogind {
            boot_id: "fixture-boot".into(),
            created_monotonic_usec: 2,
        },
    };
    let selection = SessionSelection {
        target: Some(identity.clone()),
    };
    let rows = session_rows(&shell);
    assert_eq!(selected_row(&rows, &selection), Some(1));
    shell.set_info_sort(InfoTable::Users, InfoSortCol::Name);
    let rows = session_rows(&shell);
    assert_eq!(
        selection
            .target
            .as_ref()
            .map(|identity| identity.session_id.as_str()),
        Some("2"),
        "the complete session identity never drifts"
    );
    assert_eq!(selected_row(&rows, &selection), Some(0));
    let gone = SessionSelection {
        target: Some(SessionIdentity {
            session_id: SessionId::new("404"),
            incarnation: identity.incarnation,
        }),
    };
    assert_eq!(selected_row(&rows, &gone), None);
}

#[test]
fn cursor_moves_clamp_at_the_table_bounds() {
    assert_eq!(moved_row(3, Some(0), -1), Some(0));
    assert_eq!(moved_row(3, Some(2), 1), Some(2));
    assert_eq!(moved_row(3, None, 1), Some(0), "enters at the first row");
    assert_eq!(moved_row(0, None, 1), None, "an empty table has no rows");
}

#[test]
fn empty_state_copy_separates_confirmed_empty_from_failed_source() {
    assert_eq!(empty_state_text(None), t("users.no_sessions"));
    assert_eq!(empty_state_text(Some(&[])), t("users.no_sessions"));
    let failed = empty_state_text(Some(&failed_source()));
    assert!(
        failed.contains(t("source.unavailable_title")),
        "an empty list from a FAILED source renders the typed reason: {failed}"
    );
    assert_ne!(
        failed,
        t("users.no_sessions"),
        "a failure must never read as a confirmed empty"
    );
}

fn control_outcome(
    action: SessionControlAction,
    result: Result<(), FailureKind>,
) -> SessionControlOutcome {
    SessionControlOutcome {
        request_id: LatestControlRequest::default().begin(),
        identity: SessionIdentity {
            session_id: SessionId::new("7"),
            incarnation: SessionIncarnation::LinuxLogind {
                boot_id: "fixture-boot".into(),
                created_monotonic_usec: 7,
            },
        },
        action,
        result,
    }
}

#[test]
fn feedback_line_names_action_and_target_for_both_outcomes() {
    let ok = control_outcome(SessionControlAction::Lock, Ok(()));
    assert_eq!(
        feedback_line_text(&ok),
        t("feedback.session_request_accepted")
            .replace("{action}", t("users.lock"))
            .replace("{target}", "7")
    );
    let denied = control_outcome(
        SessionControlAction::Disconnect,
        Err(FailureKind::PermissionDenied),
    );
    let failed = feedback_line_text(&denied);
    assert!(
        failed.contains(t("users.disconnect")) && failed.contains("7"),
        "the failure line names the action and the target: {failed}"
    );
    assert!(
        failed.contains(t("feedback.permission_denied")),
        "the failure reason travels with the outcome: {failed}"
    );
}

// ---- wired: fold → rows, sort click, selection, idle ----

#[test]
fn folded_rows_render_then_refresh_and_idle_frames_redraw_nothing() {
    let (mut app, events) = headless_sessions_app();
    route_to_sessions(&mut app);
    push_sessions(
        &events,
        vec![
            session_item("5", "zoe", Some("seat0"), Some("tty1")),
            session_item("2", "ada", None, None),
        ],
    );
    app.update();
    app.update();
    assert_eq!(
        row_targets(&mut app),
        [(0, "5".to_owned()), (1, "2".to_owned())],
        "rows render in provider order until a sort is picked"
    );
    assert_eq!(
        status_line(&mut app),
        format!("2 {} · provider order", t("users.sessions"))
    );

    let before = row_entities(&mut app);
    app.update();
    app.update();
    assert_eq!(
        before,
        row_entities(&mut app),
        "no fold, no repaint — idle frames redraw nothing"
    );

    push_sessions(
        &events,
        vec![
            session_item("9", "miguel", Some("seat1"), None),
            session_item("2", "ada", None, None),
            session_item("5", "zoe", Some("seat0"), Some("tty1")),
        ],
    );
    app.update();
    app.update();
    assert_eq!(
        row_targets(&mut app),
        [
            (0, "9".to_owned()),
            (1, "2".to_owned()),
            (2, "5".to_owned()),
        ],
        "the fold observer repainted the body from the new projection"
    );
}

#[test]
fn sort_click_projects_shared_order_and_keeps_selection_on_target() {
    let (mut app, events) = headless_sessions_app();
    route_to_sessions(&mut app);
    push_sessions(
        &events,
        vec![
            session_item("5", "zoe", None, None),
            session_item("2", "ada", None, None),
            session_item("9", "miguel", None, None),
        ],
    );
    app.update();
    app.update();

    // Click the middle provider-order row ("2").
    app.world_mut().trigger(SessionRowClicked(1));
    app.update();
    assert_eq!(selected_row_target(&mut app).as_deref(), Some("2"));

    app.world_mut()
        .trigger(SessionSortClicked(InfoSortCol::Name));
    app.update();
    app.update();
    assert_eq!(
        app.world().non_send::<FrontendTrack>().shell.sessions_sort,
        Some((InfoSortCol::Name, SortDir::Asc)),
        "the observer routed the click through the shell's sort entry"
    );
    assert_eq!(
        row_targets(&mut app),
        [
            (0, "2".to_owned()),
            (1, "9".to_owned()),
            (2, "5".to_owned()),
        ],
        "rows re-projected through the shared sort (by user: ada, miguel, zoe → 2, 9, 5)"
    );
    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("2"),
        "the selected target identity survived the reorder"
    );
}

#[test]
fn keyboard_moves_clamp_and_selection_clears_when_target_leaves() {
    let (mut app, events) = headless_sessions_app();
    route_to_sessions(&mut app);
    push_sessions(
        &events,
        vec![
            session_item("5", "zoe", None, None),
            session_item("2", "ada", None, None),
        ],
    );
    app.update();
    app.update();
    app.world_mut()
        .trigger(SessionSortClicked(InfoSortCol::Name));
    app.update();

    app.world_mut().trigger(SessionSelectionMoved(1));
    app.update();
    assert_eq!(selected_row_target(&mut app).as_deref(), Some("2"));
    app.world_mut().trigger(SessionSelectionMoved(9));
    app.update();
    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("5"),
        "the cursor saturates at the last row"
    );
    app.world_mut().trigger(SessionSelectionMoved(-9));
    app.update();
    assert_eq!(
        selected_row_target(&mut app).as_deref(),
        Some("2"),
        "the cursor saturates at the first row"
    );

    // The next fold drops "2" entirely.
    push_sessions(&events, vec![session_item("5", "zoe", None, None)]);
    app.update();
    app.update();
    assert_eq!(
        selected_row_target(&mut app),
        None,
        "a vanished target deselects instead of jumping to a neighbor"
    );
    assert_eq!(
        app.world().resource::<SessionSelection>().target,
        None,
        "the selection resource stays identity-keyed and honest"
    );
}

#[test]
fn sessions_table_columns_match_shared_contract() {
    let cols = super::columns();
    let labels: Vec<String> = cols.iter().map(|c| c.label.clone()).collect();
    assert_eq!(
        labels,
        vec![
            t("users.session"),
            t("common.user"),
            t("users.seat"),
            t("users.tty"),
            t("users.remote"),
            t("users.logon"),
        ],
        "table column headers must align to Session, User, Seat, TTY, Remote, Logon"
    );
}

#[test]
fn sessions_toolbar_mounts_disconnect_and_lock_buttons() {
    let (mut app, events) = headless_sessions_app();
    route_to_sessions(&mut app);
    push_sessions(
        &events,
        vec![session_item("2", "ada", Some("seat0"), Some("tty1"))],
    );
    app.update();
    app.update();

    let disconnect_buttons: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<super::SessionDisconnectButton>>()
        .iter(app.world())
        .collect();
    let lock_buttons: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<super::SessionLockButton>>()
        .iter(app.world())
        .collect();

    assert_eq!(
        disconnect_buttons.len(),
        1,
        "sessions toolbar mounts exactly one Disconnect button"
    );
    assert_eq!(
        lock_buttons.len(),
        1,
        "sessions toolbar mounts exactly one Lock button"
    );
}

#[test]
fn sessions_toolbar_buttons_arm_confirmation_for_selected_session() {
    let (mut app, events) = headless_sessions_app();
    route_to_sessions(&mut app);
    push_sessions(
        &events,
        vec![session_item("2", "ada", Some("seat0"), Some("tty1"))],
    );
    app.update();
    app.update();

    // Select row 0 ("2")
    app.world_mut().trigger(SessionRowClicked(0));
    app.update();

    let lock_button = app
        .world_mut()
        .query_filtered::<Entity, With<super::SessionLockButton>>()
        .iter(app.world())
        .next()
        .expect("lock button exists");

    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::Activate {
            entity: lock_button,
        });
    app.update();

    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .pending_confirmation()
            .is_some(),
        "activating lock button arms confirmation"
    );
}

// ---- terminal family marks (BEVY-150) ----

#[test]
fn session_kind_classifies_the_observed_terminal_family() {
    let graphical = session_item("3", "ada", Some("seat0"), Some("wayland-0"));
    assert_eq!(
        kind::classify(&graphical),
        SessionKind::Graphical,
        "a Wayland compositor terminal owns a display session"
    );
    let x11 = session_item("4", "bo", Some("seat0"), Some(":0"));
    assert_eq!(kind::classify(&x11), SessionKind::Graphical);
    let console = session_item("5", "cy", Some("seat0"), Some("tty2"));
    assert_eq!(
        kind::classify(&console),
        SessionKind::Console,
        "a physical console line owns a terminal session"
    );
    let pseudo = session_item("6", "di", None, Some("pts/3"));
    assert_eq!(kind::classify(&pseudo), SessionKind::Console);
    let mut remote = session_item("7", "ed", Some("seat0"), Some("tty1"));
    remote.remote = true;
    assert_eq!(
        kind::classify(&remote),
        SessionKind::Remote,
        "the transport outranks the terminal that carries it"
    );
    let bare = session_item("8", "fi", None, None);
    assert_eq!(
        kind::classify(&bare),
        SessionKind::Unknown,
        "an unobserved family is unknown, never a guess"
    );
}

#[test]
fn session_rows_carry_their_terminal_family_mark() {
    let (mut app, events) = headless_sessions_app();
    route_to_sessions(&mut app);
    push_sessions(
        &events,
        vec![
            session_item("2", "ada", Some("seat0"), Some("wayland-0")),
            session_item("3", "bo", None, Some("tty1")),
        ],
    );
    app.update();
    app.update();

    let marks: Vec<SessionKind> = app
        .world_mut()
        .query_filtered::<&SessionKindMarker, ()>()
        .iter(app.world())
        .map(|marker| marker.0)
        .collect();
    assert_eq!(marks.len(), 2, "every session row carries one family mark");
    assert!(
        marks.contains(&SessionKind::Graphical) && marks.contains(&SessionKind::Console),
        "the marks name the two observed families: {marks:?}"
    );
}

// ---- the bubble confirmation card (BEVY-149) ----

/// Every text under one entity, in tree order.
fn texts_under(app: &mut App, root: Entity) -> Vec<String> {
    let world = app.world_mut();
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Some(text) = world.get::<bevy::ui::widget::Text>(entity) {
            found.push(text.0.clone());
        }
        if let Some(children) = world.get::<bevy::ecs::hierarchy::Children>(entity) {
            stack.extend(children.iter().copied());
        }
    }
    found
}

/// Select the only session, then arm the logout verb.
fn arm_logout(app: &mut App) {
    app.world_mut().trigger(SessionRowClicked(0));
    app.update();
    let disconnect = app
        .world_mut()
        .query_filtered::<Entity, With<super::SessionDisconnectButton>>()
        .iter(app.world())
        .next()
        .expect("the sessions toolbar mounts a disconnect button");
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::Activate { entity: disconnect });
    app.update();
    app.update();
}

#[test]
fn the_logout_verb_mounts_a_bubble_naming_the_frozen_session() {
    let (mut app, events) = headless_sessions_app();
    route_to_sessions(&mut app);
    push_sessions(
        &events,
        vec![session_item("2", "ada", Some("seat0"), Some("tty1"))],
    );
    app.update();
    app.update();
    arm_logout(&mut app);

    let target = {
        let track = app.world().non_send::<FrontendTrack>();
        bubble::bubble_target(&track.shell).expect("the bubble echoes the armed gate")
    };
    assert_eq!(
        target.session_id, "2",
        "the frozen target is the session id"
    );
    assert_eq!(target.user, "ada", "the frozen target names its owner");
    assert_eq!(
        target.action,
        SessionControlAction::Disconnect,
        "the frozen target keeps the verb"
    );

    let bubbles: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<bubble::SessionBubble>>()
        .iter(app.world())
        .collect();
    assert_eq!(bubbles.len(), 1, "exactly one bubble card is mounted");
    let copy = texts_under(&mut app, bubbles[0]);
    assert!(
        copy.iter()
            .any(|line| line.contains("2") && line.contains("ada")),
        "the bubble names the session it would log out: {copy:?}"
    );
}

#[test]
fn the_bubble_confirm_routes_through_the_shared_typed_gate() {
    let controls = Arc::new(RecordedSessionControls::default());
    let (mut app, events) = headless_sessions_app_with_controls(controls.clone());
    route_to_sessions(&mut app);
    push_sessions(
        &events,
        vec![session_item("2", "ada", Some("seat0"), Some("tty1"))],
    );
    app.update();
    app.update();
    arm_logout(&mut app);

    let confirm = app
        .world_mut()
        .query_filtered::<Entity, With<bubble::SessionBubbleConfirm>>()
        .iter(app.world())
        .next()
        .expect("the bubble carries a confirm choice");
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::Activate { entity: confirm });
    app.update();
    app.update();

    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .pending_confirmation()
            .is_none(),
        "the shared gate is cleared once the bubble confirms"
    );
    let submitted = controls.0.lock().expect("session control recorder lock");
    assert_eq!(
        submitted.len(),
        1,
        "the shared typed confirm path submitted exactly one request"
    );
    assert_eq!(
        submitted[0].identity.session_id.as_str(),
        "2",
        "the submitted request carries the frozen session target"
    );
    assert_eq!(
        submitted[0].action,
        SessionControlAction::Disconnect,
        "the submitted request carries the frozen verb"
    );
}

#[test]
fn the_bubble_dismiss_clears_the_gate_without_an_effect() {
    let controls = Arc::new(RecordedSessionControls::default());
    let (mut app, events) = headless_sessions_app_with_controls(controls.clone());
    route_to_sessions(&mut app);
    push_sessions(
        &events,
        vec![session_item("2", "ada", Some("seat0"), Some("tty1"))],
    );
    app.update();
    app.update();
    arm_logout(&mut app);

    let dismiss = app
        .world_mut()
        .query_filtered::<Entity, With<bubble::SessionBubbleDismiss>>()
        .iter(app.world())
        .next()
        .expect("the bubble carries a dismiss choice");
    app.world_mut()
        .commands()
        .trigger(bevy::ui_widgets::Activate { entity: dismiss });
    app.update();
    app.update();

    assert!(
        app.world()
            .non_send::<FrontendTrack>()
            .shell
            .pending_confirmation()
            .is_none(),
        "dismissing the bubble clears the shared gate"
    );
    assert!(
        controls
            .0
            .lock()
            .expect("session control recorder lock")
            .is_empty(),
        "a dismissed bubble submits nothing"
    );
}
