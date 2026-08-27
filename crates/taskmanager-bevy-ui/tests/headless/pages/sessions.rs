//! test-intent: behavior
//!
//! Headless behavior tests for the Sessions page (same two layers as the
//! services page tests): the pure row view model projects through the shared
//! sessions sort with an honest seat/tty summary (missing fields render the
//! shared marker, never a fabricated empty), the id-keyed selection survives
//! re-sorts, the control-outcome feedback names action and target, and the
//! wired `MinimalPlugins` page renders folded rows, repaints only on
//! sessions-domain folds, routes header clicks through the shell's sort entry
//! and resolves clicked/moved rows ONLY through `sorted_session_at`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bevy::MinimalPlugins;
use bevy::app::App;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ui::BackgroundColor;
use taskmanager_application::i18n::t;
use taskmanager_application::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus,
    CorrelatedSessionEvent, EventEnvelope, EventPort, EventPortError, EventSequence, FailureKind,
    HostTelemetryRequest, LatestControlRequest, PartialSourceSnapshot, PlatformClient,
    PlatformEvent, PlatformEventBatch, PlatformFacets, PlatformHandle, ProviderId, RequestEnvelope,
    RequestId, RequestPort, SessionControlAction, SessionControlOutcome, SessionEvent, SessionId,
    SessionItem, SourceOutcome, SourceStatus, SubmissionError, SystemFacets,
};
use taskmanager_shell::presentation::MISSING_VALUE;
use taskmanager_shell::{InfoSortCol, InfoTable, ShellApp, SortDir};
use taskmanager_theme::Theme;

use super::{
    SessionRowClicked, SessionSelection, SessionSelectionMoved, SessionSortClicked,
    SessionsRowMarker, SessionsStatusLine, empty_state_text, feedback_line_text, moved_row,
    selected_row, session_rows, session_seat_text, session_tty_text,
};
use crate::app::{FrontendTrack, Page, Route, RouteChanged};
use crate::palette::ui_palette;
use crate::runtime::{RuntimeCache, SharedRuntime};
use crate::window::FrontendWindowPlugin;
use crate::window::tests::HeadlessFrontendPlugins;

// ---- fixtures ----

fn session_item(id: &str, user: &str, seat: Option<&str>, tty: Option<&str>) -> SessionItem {
    SessionItem {
        id: id.to_owned(),
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

fn headless_sessions_app() -> (App, Arc<ScriptedEvents>) {
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
            .with_system(SystemFacets::default().with_host(Arc::new(QuietRequests))),
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
        palette: ui_palette(&Theme::dark()),
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
    app.world_mut()
        .commands()
        .trigger(RouteChanged(Page::Sessions));
}

fn row_targets(app: &mut App) -> Vec<(usize, String)> {
    app.world_mut()
        .query_filtered::<&SessionsRowMarker, ()>()
        .iter(app.world())
        .map(|marker| (marker.0, marker.1.clone()))
        .collect()
}

fn row_entities(app: &mut App) -> Vec<(Entity, String)> {
    app.world_mut()
        .query_filtered::<(Entity, &SessionsRowMarker), ()>()
        .iter(app.world())
        .map(|(entity, marker)| (entity, marker.1.clone()))
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
    let palette = ui_palette(&Theme::dark());
    let highlight = palette.nav_active_bg.to_srgba();
    app.world_mut()
        .query_filtered::<(&SessionsRowMarker, &BackgroundColor), ()>()
        .iter(app.world())
        .find(|(_, fill)| fill.0.to_srgba() == highlight)
        .map(|(marker, _)| marker.1.clone())
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
        .map(|row| row.target)
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
    let selection = SessionSelection {
        target: Some("2".to_owned()),
    };
    let rows = session_rows(&shell);
    assert_eq!(selected_row(&rows, &selection), Some(1));
    shell.set_info_sort(InfoTable::Users, InfoSortCol::Name);
    let rows = session_rows(&shell);
    assert_eq!(
        selection.target.as_deref(),
        Some("2"),
        "the target id never drifts"
    );
    assert_eq!(selected_row(&rows, &selection), Some(0));
    let gone = SessionSelection {
        target: Some("404".to_owned()),
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
        session_id: SessionId::new("7"),
        action,
        result,
    }
}

#[test]
fn feedback_line_names_action_and_target_for_both_outcomes() {
    let ok = control_outcome(SessionControlAction::Lock, Ok(()));
    assert_eq!(
        feedback_line_text(&ok),
        t("feedback.action_succeeded")
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
        "the selected target id survived the reorder"
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
        "the selection resource stays id-keyed and honest"
    );
}
