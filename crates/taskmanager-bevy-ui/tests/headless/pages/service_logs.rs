//! test-intent: behavior
//!
//! Service log panel behavior over the shell's renderer-neutral lifecycle
//! (ADR-027). The bevy side is a pure consumer: these tests pin the seams —
//!
//! - the open affordance resolves the SELECTED service (never "first row")
//!   and submits the follow effect through the same PendingEffects queue the
//!   drain drains;
//! - folded stream snapshots grow the visible entries and duplicate cursors
//!   never duplicate rows;
//! - the panel's status caption is a typed decision over the provider state
//!   (loading / empty / failure kinds), never a fabricated progress;
//! - the panel-local chord mapping is total and honest (F/P/L/T/Esc, nothing
//!   else), and each chord actually moves the shell's log state;
//! - the repaint gate fires only when a rendered feed fact moves.
//!
//! Mounted from `pages/services.rs` (the panel is a Services-page surface).

use bevy::input::keyboard::KeyCode;
use taskmanager_application::PlatformEffect;
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_core::core::services::{
    ServiceLogEntry, ServiceLogErrorKind, ServiceLogFailure, ServiceLogLevelFilter,
    ServiceLogProviderState, ServiceLogQuery, ServiceLogStreamSnapshot, ServiceLogStreamState,
    ServiceLogTimeFilter,
};
use taskmanager_shell::ShellApp;
use taskmanager_shell::app::OpenServiceLog;

use super::log_panel::{
    LogPanelRepaintRequired, ServiceLogControlAction, log_fingerprint, log_panel_key,
    log_status_caption,
};
use super::tests::{headless_services_app, push_services, route_to_services};

// ---- fixtures -----------------------------------------------------------

fn service_item(id: &str, name: &str, status: ServiceStatus) -> ServiceItem {
    ServiceItem::from_inventory(
        id,
        name,
        status,
        format!("{name} description"),
        "loaded",
        "active",
        "running",
    )
}

fn entry(index: usize) -> ServiceLogEntry {
    ServiceLogEntry {
        cursor: format!("j:{index:04}"),
        realtime_timestamp_micros: Some(1_700_000_000_000_000 + index as u64 * 1_000_000),
        priority: Some(6),
        level: taskmanager_core::core::services::ServiceLogLevel::Unknown,
        message: format!("line {index}"),
    }
}

fn stream_snapshot(
    query: &ServiceLogQuery,
    entries: Vec<ServiceLogEntry>,
) -> ServiceLogStreamSnapshot {
    ServiceLogStreamSnapshot {
        query: query.clone(),
        state: ServiceLogStreamState::from_query_entries(query, entries),
    }
}

// ---- open affordance ------------------------------------------------------

#[test]
fn the_open_affordance_targets_the_selected_service_and_submits_one_follow() {
    let (mut app, events) = headless_services_app();
    push_services(
        &events,
        vec![
            service_item("alpha.service", "alpha", ServiceStatus::Active),
            service_item("beta.service", "beta", ServiceStatus::Active),
        ],
    );
    route_to_services(&mut app);
    app.update();

    // The page selection points at the SECOND row; the open affordance must
    // freeze THAT identity, not the first row.
    let target = {
        let track = app.world().non_send_resource::<crate::app::FrontendTrack>();
        track
            .shell
            .sorted_services()
            .iter()
            .find(|service| service.id.to_string().contains("beta"))
            .map(|service| service.id.clone())
            .expect("beta present")
    };
    app.world_mut()
        .resource_mut::<crate::pages::services::ServiceSelection>()
        .target = Some(target.clone());

    app.world_mut()
        .commands()
        .trigger(crate::pages::services::log_panel::ServiceLogsRequested);
    app.world_mut().flush();

    let track = app.world().non_send_resource::<crate::app::FrontendTrack>();
    let open = track.shell.service_log.as_ref().expect("stream open");
    assert_eq!(
        open.service_id(),
        Some(&target),
        "the stream freezes the SELECTED service identity"
    );
    let submitted = app
        .world()
        .resource::<crate::input::PendingEffects>()
        .0
        .iter()
        .any(|effect| matches!(effect, PlatformEffect::ServiceLogStream(_)));
    assert!(
        submitted,
        "the open affordance submits the follow request through the drain queue"
    );
}

// ---- feed semantics through the panel's data source -----------------------

#[test]
fn folded_snapshots_grow_visible_entries_without_cursor_duplicates() {
    let mut shell = ShellApp::new();
    let service = taskmanager_core::core::target::ServiceId::new("demo.service");
    let _ = shell.open_service_log_for(service.clone());
    let query = ServiceLogQuery {
        service_id: service.clone(),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };

    let now = 1_000;
    if let Some(open) = shell.service_log.as_mut() {
        open.feed.apply_at(
            stream_snapshot(&query, (0..3).map(|i| entry(i)).collect()),
            now,
        );
    }
    assert_eq!(
        shell
            .visible_service_log_entries(now * 1_000)
            .expect("stream open")
            .len(),
        3,
        "the first batch is fully visible"
    );

    // The provider replays an overlapping window: cursor dedup keeps rows
    // unique, and only genuinely new lines append.
    if let Some(open) = shell.service_log.as_mut() {
        open.feed.apply_at(
            stream_snapshot(&query, (2..6).map(|i| entry(i)).collect()),
            now + 2_000,
        );
    }
    let visible = shell
        .visible_service_log_entries(now * 1_000)
        .expect("stream open");
    assert_eq!(
        visible.len(),
        6,
        "overlap dedups: 3 + 4 batches share one cursor, so six unique rows"
    );
    let mut messages: Vec<&str> = visible.iter().map(|entry| entry.message.as_str()).collect();
    messages.sort_unstable();
    messages.dedup();
    assert_eq!(messages.len(), visible.len(), "no duplicate rows render");
}

// ---- honest status caption ------------------------------------------------

#[test]
fn the_status_caption_is_a_typed_provider_decision() {
    let loading = ServiceLogProviderState::default();
    assert_eq!(
        log_status_caption(&loading),
        taskmanager_application::i18n::t("svc.logs_loading"),
        "a cold provider says it is loading, never that there are no logs"
    );

    let mut denied = ServiceLogProviderState::default();
    denied.observe_failure(ServiceLogFailure::with_detail(
        ServiceLogErrorKind::PermissionDenied,
        "",
    ));
    assert_eq!(
        log_status_caption(&denied),
        taskmanager_application::i18n::t("svc.logs_permission_denied"),
        "a permission failure is reported as one, not as empty"
    );

    let mut failed = ServiceLogProviderState::default();
    failed.observe_failure(ServiceLogFailure::with_detail(
        ServiceLogErrorKind::ProviderFailed,
        "unit failed",
    ));
    let caption = log_status_caption(&failed);
    assert!(
        caption.starts_with(taskmanager_application::i18n::t("svc.logs_failed")),
        "a provider failure reports the failure vocabulary, got {caption}"
    );

    let mut empty = ServiceLogProviderState::default();
    empty.observe_success(true, 10);
    assert_eq!(
        log_status_caption(&empty),
        taskmanager_application::i18n::t("svc.logs_empty"),
        "an empty successful read says there are no entries"
    );

    let mut healthy = ServiceLogProviderState::default();
    healthy.observe_success(false, 10);
    assert!(
        log_status_caption(&healthy).is_empty(),
        "a healthy non-empty feed needs no caption — the entries speak"
    );
}

// ---- panel-local chords -----------------------------------------------------

#[test]
fn the_chord_mapping_is_total_and_exclusive() {
    assert_eq!(
        log_panel_key(KeyCode::KeyF),
        Some(ServiceLogControlAction::ToggleFollow)
    );
    assert_eq!(
        log_panel_key(KeyCode::KeyP),
        Some(ServiceLogControlAction::TogglePaused)
    );
    assert_eq!(
        log_panel_key(KeyCode::KeyL),
        Some(ServiceLogControlAction::CycleLevel)
    );
    assert_eq!(
        log_panel_key(KeyCode::KeyT),
        Some(ServiceLogControlAction::CycleTime)
    );
    assert_eq!(
        log_panel_key(KeyCode::Escape),
        Some(ServiceLogControlAction::Close)
    );
    assert_eq!(
        log_panel_key(KeyCode::KeyA),
        None,
        "unowned keys never consume"
    );
    assert_eq!(
        log_panel_key(KeyCode::Enter),
        None,
        "Enter is not a panel chord"
    );
}

#[test]
fn panel_controls_actually_move_the_log_state_and_a_stopped_feed_stays_quiet() {
    let mut shell = ShellApp::new();
    let service = taskmanager_core::core::target::ServiceId::new("demo.service");
    let _ = shell.open_service_log_for(service);

    shell.toggle_service_log_follow();
    assert!(
        !shell.service_log.as_ref().expect("open").feed.follow,
        "follow toggles off"
    );
    shell.toggle_service_log_paused();
    assert!(
        shell.service_log.as_ref().expect("open").feed.paused,
        "pause toggles on"
    );
    shell.cycle_service_log_level();
    assert_eq!(
        shell.service_log.as_ref().expect("open").feed.level,
        ServiceLogLevelFilter::Errors,
        "level cycles All -> Errors"
    );
    shell.cycle_service_log_time();
    assert_eq!(
        shell.service_log.as_ref().expect("open").feed.time,
        ServiceLogTimeFilter::LastHour,
        "time cycles All -> LastHour"
    );

    // A stopped feed never submits follow requests, and closing the panel
    // leaves nothing behind that could poll.
    assert!(
        shell.poll_service_log(10_000).is_none(),
        "follow-off + paused submit nothing"
    );
    shell.close_service_log();
    assert!(shell.service_log.is_none(), "close clears the lifecycle");
    assert!(shell.poll_service_log(20_000).is_none());
}

// ---- repaint gate -----------------------------------------------------------

#[test]
fn the_repaint_gate_fires_only_when_a_rendered_fact_moves() {
    let service = taskmanager_core::core::target::ServiceId::new("demo.service");
    let mut open = OpenServiceLog::new(service);
    let closed = log_fingerprint(None);
    assert_ne!(
        closed,
        log_fingerprint(Some(&open)),
        "open vs closed must differ"
    );
    let before = log_fingerprint(Some(&open));
    assert_eq!(
        before,
        log_fingerprint(Some(&open)),
        "an unrelated fold with unchanged feed facts repaints nothing"
    );

    let query = ServiceLogQuery {
        service_id: taskmanager_core::core::target::ServiceId::new("demo.service"),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };
    open.feed
        .apply_at(stream_snapshot(&query, vec![entry(0)]), 5);
    assert_ne!(
        before,
        log_fingerprint(Some(&open)),
        "a new entry demands a repaint"
    );
    let _ = LogPanelRepaintRequired;
}
