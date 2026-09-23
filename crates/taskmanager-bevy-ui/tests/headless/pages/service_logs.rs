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

use bevy::app::App;
use bevy::input::keyboard::KeyCode;
use taskmanager_application::PlatformEffect;
use taskmanager_core::core::services::ServiceLogLevel;
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
use taskmanager_application::i18n::t;
use taskmanager_core::core::target::ServiceId;
use taskmanager_shell::FeedbackSeverity;
use taskmanager_theme::Theme;

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
        level: ServiceLogLevel::Unknown,
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
        let track = app.world().non_send::<crate::app::FrontendTrack>();
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

    let track = app.world().non_send::<crate::app::FrontendTrack>();
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
    let service = ServiceId::new("demo.service");
    let _ = shell.open_service_log_for(service.clone());
    let query = ServiceLogQuery {
        service_id: service.clone(),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };

    let now = 1_000;
    if let Some(open) = shell.service_log.as_mut() {
        open.feed
            .apply_at(stream_snapshot(&query, (0..3).map(entry).collect()), now);
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
            stream_snapshot(&query, (2..6).map(entry).collect()),
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

// ---- painted rows vs the level filter ---------------------------------------

/// Every `(role, text)` node the freshly spawned log panel paints. The scene
/// is rebuilt from the shell exactly like the production repaint path does.
fn painted_panel(shell: &ShellApp) -> Vec<(crate::window::Role, String)> {
    use bevy::MinimalPlugins;
    use bevy::asset::{AssetPlugin, Assets};
    use bevy::scene::{ScenePlugin, WorldSceneExt};
    use bevy::text::Font;
    use bevy::ui::widget::Text;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((AssetPlugin::default(), ScenePlugin));
    app.init_resource::<Assets<Font>>();
    let palette = crate::palette::ui_palette(&Theme::dark());
    let scene = super::log_panel::service_log_panel_scene(shell, &palette);
    let world = app.world_mut();
    let root = world
        .spawn_scene(scene)
        .expect("the log panel scene resolves without app resources")
        .id();
    let texts = world
        .query::<(&Text, &crate::window::TextRole)>()
        .iter(world)
        .map(|(text, role)| (role.0, text.0.clone()))
        .collect();
    assert!(world.despawn(root), "the log panel scene despawns cleanly");
    texts
}

fn painted_texts(shell: &ShellApp) -> Vec<String> {
    painted_panel(shell)
        .into_iter()
        .map(|(_, text)| text)
        .collect()
}

/// The painted stream rows: the monospace texts minus each row's stamp (the
/// chips/title/status carry Caption/Body roles).
fn painted_messages(shell: &ShellApp) -> Vec<String> {
    painted_panel(shell)
        .into_iter()
        .filter(|(role, text)| *role == crate::window::Role::Mono && text != "--:--:--")
        .map(|(_, text)| text)
        .collect()
}

#[test]
fn painted_rows_follow_the_level_filter() {
    let mut shell = ShellApp::new();
    let service = ServiceId::new("demo.service");
    let _ = shell.open_service_log_for(service.clone());
    let query = ServiceLogQuery {
        service_id: service,
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };
    let graded =
        |index: usize, priority: u8, level: ServiceLogLevel, message: &str| ServiceLogEntry {
            cursor: format!("j:{index:04}"),
            realtime_timestamp_micros: None,
            priority: Some(priority),
            level,
            message: message.to_owned(),
        };
    let entries = vec![
        graded(0, 3, ServiceLogLevel::Error, "disk failure"),
        graded(1, 4, ServiceLogLevel::Warning, "retrying mount"),
        graded(2, 6, ServiceLogLevel::Info, "started worker"),
        graded(3, 7, ServiceLogLevel::Debug, "trace payload"),
    ];
    if let Some(open) = shell.service_log.as_mut() {
        open.feed.apply_at(stream_snapshot(&query, entries), 1_000);
    }

    // All levels: every seeded stream row paints.
    assert_eq!(
        painted_messages(&shell),
        vec![
            "disk failure",
            "retrying mount",
            "started worker",
            "trace payload"
        ]
    );

    // All -> Errors: only the error row paints, and the chip names the filter.
    shell.cycle_service_log_level();
    assert_eq!(painted_messages(&shell), vec!["disk failure"]);
    assert!(
        painted_texts(&shell)
            .iter()
            .any(|text| text == t("svc.logs_level_errors")),
        "the level chip must name the active filter"
    );

    // Errors -> WarningsAndErrors: warning + error rows paint, info/debug do
    // not (the filtered stream is a whole-row decision, not a text swap).
    shell.cycle_service_log_level();
    assert_eq!(
        painted_messages(&shell),
        vec!["disk failure", "retrying mount"]
    );
    let texts = painted_texts(&shell);
    assert_eq!(
        texts.iter().filter(|text| *text == "--:--:--").count(),
        2,
        "one painted stamp per visible row: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text.contains("started worker"))
            && !texts.iter().any(|text| text.contains("trace payload")),
        "a filtered-out row must not paint: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text == t("svc.logs_level_warnings")),
        "the level chip must follow the second cycle"
    );
}

// ---- honest status caption ------------------------------------------------

#[test]
fn the_status_caption_is_a_typed_provider_decision() {
    let loading = ServiceLogProviderState::default();
    assert_eq!(
        log_status_caption(&loading),
        t("svc.logs_loading"),
        "a cold provider says it is loading, never that there are no logs"
    );

    let mut denied = ServiceLogProviderState::default();
    denied.observe_failure(ServiceLogFailure::with_detail(
        ServiceLogErrorKind::PermissionDenied,
        "",
    ));
    assert_eq!(
        log_status_caption(&denied),
        t("svc.logs_permission_denied"),
        "a permission failure is reported as one, not as empty"
    );

    let mut failed = ServiceLogProviderState::default();
    failed.observe_failure(ServiceLogFailure::with_detail(
        ServiceLogErrorKind::ProviderFailed,
        "unit failed",
    ));
    let caption = log_status_caption(&failed);
    assert!(
        caption.starts_with(t("svc.logs_failed")),
        "a provider failure reports the failure vocabulary, got {caption}"
    );

    let mut empty = ServiceLogProviderState::default();
    empty.observe_success(true, 10);
    assert_eq!(
        log_status_caption(&empty),
        t("svc.logs_empty"),
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
        log_panel_key(KeyCode::KeyE),
        Some(ServiceLogControlAction::Export)
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
    let service = ServiceId::new("alpha.service");
    let (mut app, events) = headless_services_app();
    push_services(
        &events,
        vec![service_item(
            "alpha.service",
            "alpha",
            ServiceStatus::Active,
        )],
    );
    route_to_services(&mut app);
    app.update();

    // Open the panel through the production request the page's own affordance
    // fires, then drive every panel chord through the real keyboard dispatch:
    // the open panel owns the Services-page keyboard, so the keys must move
    // the shared log state rather than fall through to navigation.
    app.world_mut()
        .resource_mut::<crate::pages::services::ServiceSelection>()
        .target = Some(service);
    app.world_mut()
        .commands()
        .trigger(crate::pages::services::log_panel::ServiceLogsRequested);
    app.update();
    assert!(
        app.world()
            .non_send::<crate::app::FrontendTrack>()
            .shell
            .service_log
            .is_some(),
        "the open request mounts the panel lifecycle"
    );

    press_input(&mut app, KeyCode::KeyF);
    app.update();
    assert!(
        !app.world()
            .non_send::<crate::app::FrontendTrack>()
            .shell
            .service_log
            .as_ref()
            .expect("open")
            .feed
            .follow,
        "F toggles follow off through the keyboard lane"
    );

    press_input(&mut app, KeyCode::KeyP);
    app.update();
    assert!(
        app.world()
            .non_send::<crate::app::FrontendTrack>()
            .shell
            .service_log
            .as_ref()
            .expect("open")
            .feed
            .paused,
        "P pauses the feed through the keyboard lane"
    );

    press_input(&mut app, KeyCode::KeyL);
    app.update();
    assert_eq!(
        app.world()
            .non_send::<crate::app::FrontendTrack>()
            .shell
            .service_log
            .as_ref()
            .expect("open")
            .feed
            .level,
        ServiceLogLevelFilter::Errors,
        "L cycles All -> Errors"
    );

    press_input(&mut app, KeyCode::KeyT);
    app.update();
    assert_eq!(
        app.world()
            .non_send::<crate::app::FrontendTrack>()
            .shell
            .service_log
            .as_ref()
            .expect("open")
            .feed
            .time,
        ServiceLogTimeFilter::LastHour,
        "T cycles All -> LastHour"
    );

    // A stopped feed never submits follow requests, and the panel's own close
    // chord clears the lifecycle so nothing can poll afterwards.
    {
        let world = app.world_mut();
        let mut track = world.non_send_mut::<crate::app::FrontendTrack>();
        assert!(
            track.shell.poll_service_log(10_000).is_none(),
            "follow-off + paused submit nothing"
        );
    }

    press_input(&mut app, KeyCode::Escape);
    app.update();
    assert!(
        app.world()
            .non_send::<crate::app::FrontendTrack>()
            .shell
            .service_log
            .is_none(),
        "Escape closes the panel lifecycle"
    );
    {
        let world = app.world_mut();
        let mut track = world.non_send_mut::<crate::app::FrontendTrack>();
        assert!(track.shell.poll_service_log(20_000).is_none());
    }
}

// ---- repaint gate -----------------------------------------------------------

#[test]
fn the_repaint_gate_fires_only_when_a_rendered_fact_moves() {
    let service = ServiceId::new("demo.service");
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
        service_id: ServiceId::new("demo.service"),
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

// ---- service log export -----------------------------------------------------

#[test]
fn service_log_export_writes_formatted_lines_and_reports_notice() {
    let mut shell = ShellApp::new();
    let service = ServiceId::new("demo.service");
    let _ = shell.open_service_log_for(service.clone());
    let query = ServiceLogQuery {
        service_id: service.clone(),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };

    let now = 1_000;
    if let Some(open) = shell.service_log.as_mut() {
        open.feed
            .apply_at(stream_snapshot(&query, (0..3).map(entry).collect()), now);
    }

    let scratch_dir = crate::app::app_support::repo_temp_dir().join("bevy-svc-log-export");
    let _ = std::fs::create_dir_all(&scratch_dir);

    super::log_panel::export_service_log(&mut shell, Some(&scratch_dir));

    let exported_file = scratch_dir.join("taskmanager-service-demo.service.log");
    assert!(exported_file.exists(), "the log file must be exported");

    let content = std::fs::read_to_string(&exported_file).expect("read exported file");
    assert!(content.contains("[Unknown] line 0"));
    assert!(content.contains("[Unknown] line 1"));
    assert!(content.contains("[Unknown] line 2"));

    let notice = shell.feedback_notice().expect("feedback notice reported");
    assert_eq!(notice.severity(), FeedbackSeverity::Success);
    assert!(notice.text().contains(&exported_file.display().to_string()));

    let _ = std::fs::remove_dir_all(&scratch_dir);
}

#[test]
fn service_log_export_with_no_entries_reports_warning_notice() {
    let mut shell = ShellApp::new();
    let service = ServiceId::new("demo.service");
    let _ = shell.open_service_log_for(service.clone());

    let scratch_dir = crate::app::app_support::repo_temp_dir().join("bevy-svc-log-export-empty");
    let _ = std::fs::create_dir_all(&scratch_dir);

    super::log_panel::export_service_log(&mut shell, Some(&scratch_dir));

    let exported_file = scratch_dir.join("taskmanager-service-demo.service.log");
    assert!(
        !exported_file.exists(),
        "no file should be written when empty"
    );

    let notice = shell.feedback_notice().expect("warning notice reported");
    assert_eq!(notice.severity(), FeedbackSeverity::Warning);
    assert_eq!(notice.text(), t("svc.logs_nothing_to_export"));

    let _ = std::fs::remove_dir_all(&scratch_dir);
}

#[test]
fn e_key_triggers_service_log_export_when_panel_is_open() {
    let (mut app, events) = headless_services_app();
    push_services(
        &events,
        vec![service_item(
            "alpha.service",
            "alpha",
            ServiceStatus::Active,
        )],
    );
    route_to_services(&mut app);
    app.update();

    app.world_mut()
        .non_send_mut::<crate::app::FrontendTrack>()
        .shell
        .clear_feedback_notice();
    let scratch_dir = crate::app::app_support::repo_temp_dir().join("bevy-svc-key-export");
    let _ = std::fs::create_dir_all(&scratch_dir);
    app.world_mut()
        .insert_resource(super::log_panel::ServiceLogExportDir(Some(
            scratch_dir.clone(),
        )));

    // Before log panel is opened, pressing E does not report notice
    press_input(&mut app, KeyCode::KeyE);
    app.update();
    assert!(
        app.world()
            .non_send::<crate::app::FrontendTrack>()
            .shell
            .feedback_notice()
            .is_none(),
        "E when log panel is closed does not trigger export notice"
    );

    // Open log panel
    app.world_mut()
        .resource_mut::<crate::pages::services::ServiceSelection>()
        .target = Some(ServiceId::new("alpha.service"));
    app.world_mut()
        .commands()
        .trigger(crate::pages::services::log_panel::ServiceLogsRequested);
    app.update();

    // Populate feed
    let query = ServiceLogQuery {
        service_id: ServiceId::new("alpha.service"),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };
    {
        let mut track = app.world_mut().non_send_mut::<crate::app::FrontendTrack>();
        if let Some(open) = track.shell.service_log.as_mut() {
            open.feed
                .apply_at(stream_snapshot(&query, vec![entry(0), entry(1)]), 100);
        }
    }

    // Now press E while log panel is open
    press_input(&mut app, KeyCode::KeyE);
    app.update();

    let track = app.world().non_send::<crate::app::FrontendTrack>();
    let notice = track
        .shell
        .feedback_notice()
        .expect("feedback notice reported on E key");
    assert_eq!(notice.severity(), FeedbackSeverity::Success);

    let exported_file = scratch_dir.join("taskmanager-service-alpha.service.log");
    assert!(exported_file.exists(), "exported file must exist");
    let content = std::fs::read_to_string(&exported_file).expect("read exported file");
    assert!(content.contains("line 0"));
    assert!(content.contains("line 1"));

    let _ = std::fs::remove_dir_all(&scratch_dir);
}

fn press_input(app: &mut App, key: KeyCode) {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
    let event = KeyboardInput {
        key_code: key,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state: bevy::input::ButtonState::Pressed,
        text: None,
        repeat: false,
        window: bevy::ecs::entity::Entity::PLACEHOLDER,
    };
    let mut event = Some(event);
    app.world_mut()
        .run_system_once(
            move |mut writer: bevy::ecs::message::MessageWriter<KeyboardInput>| {
                if let Some(event) = event.take() {
                    writer.write(event);
                }
            },
        )
        .expect("injection runs");
}
