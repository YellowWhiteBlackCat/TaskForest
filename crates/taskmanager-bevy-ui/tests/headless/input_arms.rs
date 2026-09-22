//! test-intent: behavior
//!
//! Dedicated behavior tests for the keyboard adapter's dispatch arms that the
//! structural split left without one: `0b` service-log panel, `2b` inventory
//! action-menu open, `2c` SMART self-test, and `3a` inventory table motion.
//!
//! Every test drives the REAL production dispatch path — key message → arm
//! chain → shell / route / modal state — and asserts the user-visible
//! consequence. The priority tests prove which arm owned the press by checking
//! the outcome the higher-precedence arm would have produced instead (route
//! moved, gate confirmed, page row moved).
//!
//! Path-mounted from `tests/headless/input.rs` (the real-input seam suite), so
//! both files stay inside the per-file source budget.

use bevy::app::App;
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::ResMut;
use bevy::input::keyboard::KeyCode;
use taskmanager_application::{
    AppAction, AppPage, ConfirmationKind, PlatformEffect, SmartControlRequest,
};
use taskmanager_core::core::DeviceGeneration;
use taskmanager_core::core::metrics::{DiskMetrics, SystemSnapshot};
use taskmanager_core::core::services::{
    ServiceAction, ServiceItem, ServiceLogEntry, ServiceLogFeed, ServiceLogLevel,
    ServiceLogLevelFilter, ServiceLogQuery, ServiceLogStreamSnapshot, ServiceLogStreamState,
    ServiceLogTimeFilter, ServiceStatus,
};
use taskmanager_core::core::smart::SmartSelfTestKind;
use taskmanager_core::core::startup::StartupEntryId;
use taskmanager_core::core::target::{ServiceId, SessionId};

use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture;

use super::{input_app, press, press_ctrl};
use crate::app::{FrontendTrack, Page, Route, RouteChanged};
use crate::input::{PendingEffects, ShellInteractionApplied};
use crate::pages::performance::{PerformanceDeviceFocus, PerformanceDeviceTarget};
use crate::pages::services::log_panel::{LogPanelRepaintRequired, ServiceLogExportDir};
use crate::pages::services::{ServiceSelection, menu::ServiceMenuModal};
use crate::pages::sessions::SessionSelection;
use crate::pages::sessions::menu::SessionMenuModal;
use crate::pages::startup::StartupSelection;
use crate::pages::startup::menu::StartupMenuModal;

// ---- fixtures, harness, and consequence readers ----------------------------

/// Frame signals the arms publish: the re-render trigger every applied press
/// fires, and the service-log panel's single repaint trigger.
#[derive(Resource, Default)]
struct AppliedSignals {
    applied: usize,
    log_repaints: usize,
}

fn count_applied(_applied: On<ShellInteractionApplied>, mut signals: ResMut<AppliedSignals>) {
    signals.applied += 1;
}

fn count_log_repaints(_repaint: On<LogPanelRepaintRequired>, mut signals: ResMut<AppliedSignals>) {
    signals.log_repaints += 1;
}

fn shell_of(app: &App) -> &ShellApp {
    &app.world().non_send::<FrontendTrack>().shell
}

fn signals(app: &App) -> &AppliedSignals {
    app.world().resource::<AppliedSignals>()
}

/// A unique scratch directory under the repository's ignored `.tmp/`.
fn scratch_dir() -> std::path::PathBuf {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the crate lives under crates/")
        .join(".tmp")
        .join("test-scratch");
    let unique = root.join(format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&unique).expect("create the scratch directory");
    unique
}

fn service_row(id: &str, name: &str) -> ServiceItem {
    ServiceItem::from_inventory(
        id,
        name,
        ServiceStatus::Active,
        format!("{name} service"),
        "loaded",
        "active",
        "running",
    )
}

fn disk_row(device_id: &str, name: &str, model: &str) -> DiskMetrics {
    let mut disk = DiskMetrics::new(name);
    disk.device_id = device_id.to_owned();
    disk.model = model.to_owned();
    disk.device_generation = DeviceGeneration::new(1);
    disk
}

/// A shell on the Services page with two sorted service rows.
fn services_shell() -> ShellApp {
    let mut shell = ShellApp::new();
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Services));
    fixture::seed_projection_fact(
        &mut shell,
        fixture::ProjectionSeedFact::Services(Some(vec![
            service_row("svc-b", "beta"),
            service_row("svc-a", "alpha"),
        ])),
    );
    shell
}

/// The real input app routed to `page` before its first frame, so no other
/// page mounts.
fn routed_app(shell: ShellApp, page: Page) -> App {
    let mut app = input_app(shell);
    app.init_resource::<AppliedSignals>();
    app.add_observer(count_applied);
    app.add_observer(count_log_repaints);
    route_to(&mut app, page);
    app.update();
    app.update();
    app
}

/// The route protocol every transition shares (see `crate::app`).
fn route_to(app: &mut App, page: Page) {
    app.world_mut().resource_mut::<Route>().page = page;
    app.world_mut().commands().trigger(RouteChanged);
}

fn open_log(app: &mut App, service_id: &str) {
    let mut track = app.world_mut().non_send_mut::<FrontendTrack>();
    let _ = track.shell.open_service_log_for(ServiceId::new(service_id));
}

/// Fold provider log lines into the open feed — the same typed state a real
/// stream batch delivers.
fn seed_log_entries(app: &mut App, service_id: &str, messages: &[&str]) {
    let query = ServiceLogQuery {
        service_id: ServiceId::new(service_id),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };
    let entries = messages
        .iter()
        .enumerate()
        .map(|(index, message)| ServiceLogEntry {
            cursor: format!("j:{index}"),
            realtime_timestamp_micros: Some(1_700_000_000_000_000 + index as u64),
            priority: Some(6),
            level: ServiceLogLevel::Info,
            message: (*message).to_owned(),
        })
        .collect::<Vec<_>>();
    let state = ServiceLogStreamState::from_query_entries(&query, entries);
    let shell = &mut app.world_mut().non_send_mut::<FrontendTrack>().shell;
    if let Some(open) = shell.service_log.as_mut() {
        open.feed.apply_at(
            ServiceLogStreamSnapshot {
                query: query.clone(),
                state,
            },
            1_000,
        );
    }
}

fn feed(app: &App) -> &ServiceLogFeed {
    &shell_of(app)
        .service_log
        .as_ref()
        .expect("the log stream is open")
        .feed
}

fn sorted_service_ids(app: &App) -> Vec<String> {
    shell_of(app)
        .sorted_services()
        .iter()
        .map(|row| row.id.as_str().to_owned())
        .collect()
}

fn sorted_startup_ids(app: &App) -> Vec<String> {
    shell_of(app)
        .sorted_startup_entries()
        .iter()
        .map(|row| row.id.as_str().to_owned())
        .collect()
}

fn sorted_session_ids(app: &App) -> Vec<String> {
    shell_of(app)
        .sorted_sessions()
        .iter()
        .map(|row| row.id.as_str().to_owned())
        .collect()
}

fn service_menu_id(app: &App) -> Option<String> {
    app.world()
        .resource::<ServiceMenuModal>()
        .session
        .as_ref()
        .map(|session| session.frozen.0.id.as_str().to_owned())
}

fn startup_menu_id(app: &App) -> Option<String> {
    app.world()
        .resource::<StartupMenuModal>()
        .session
        .as_ref()
        .map(|session| session.frozen.0.id.as_str().to_owned())
}

fn session_menu_id(app: &App) -> Option<String> {
    app.world()
        .resource::<SessionMenuModal>()
        .session
        .as_ref()
        .map(|session| session.frozen.0.id.as_str().to_owned())
}

fn page_target(app: &App) -> Option<String> {
    app.world()
        .resource::<ServiceSelection>()
        .target
        .as_ref()
        .map(|id| id.as_str().to_owned())
}

fn smart_target(app: &App) -> Option<String> {
    shell_of(app)
        .pending_smart_self_test()
        .map(|intent| intent.device_id.as_str().to_owned())
}

/// Install a snapshot that carries exactly `disks` — the same fixture seam a
/// real correlated batch folds through.
fn install_disks(app: &mut App, disks: Vec<DiskMetrics>) {
    let mut track = app.world_mut().non_send_mut::<FrontendTrack>();
    fixture::edit_snapshot(&mut track.shell, |snapshot| {
        *snapshot = Some(SystemSnapshot {
            disks,
            ..Default::default()
        });
    });
}

/// Arm 0b — the service log panel's chords, its ownership of the Services
/// page keyboard, and the panel repaint signal each chord publishes.
#[test]
fn arm_0b_service_log_chords_own_the_services_page() {
    let mut app = routed_app(services_shell(), Page::Services);
    let scratch = scratch_dir();
    app.world_mut().resource_mut::<ServiceLogExportDir>().0 = Some(scratch.clone());
    let target = sorted_service_ids(&app)[0].clone();
    open_log(&mut app, &target);
    seed_log_entries(&mut app, &target, &["line one", "line two"]);
    let applied_before = signals(&app).applied;

    // Bare `p` is the panel's pause chord; the frontend navigation router
    // would otherwise move the route to Settings.
    press(&mut app, KeyCode::KeyP, Some("p"));
    app.update();
    assert!(feed(&app).paused, "P pauses the open stream");
    assert_eq!(
        app.world().resource::<Route>().page,
        Page::Services,
        "the route chord must not steal P from the panel owner"
    );

    press(&mut app, KeyCode::KeyF, Some("f"));
    app.update();
    assert!(!feed(&app).follow, "F toggles follow off");

    // `e` exports the filtered rows into the panel's export directory.
    press(&mut app, KeyCode::KeyE, Some("e"));
    app.update();
    let exported = scratch.join(format!("taskmanager-service-{target}.log"));
    assert!(
        exported.is_file(),
        "E writes the filtered log to the export dir, got {exported:?}"
    );
    assert!(
        shell_of(&app)
            .feedback_text()
            .contains("taskmanager-service-"),
        "the export outcome is reported, got {:?}",
        shell_of(&app).feedback_text()
    );

    // Enter is not a panel chord, but the panel still owns the page: the
    // inventory Enter arm (2b) must not open a menu underneath it.
    press(&mut app, KeyCode::Enter, None);
    app.update();
    assert_eq!(
        service_menu_id(&app),
        None,
        "no action menu opens under the panel owner"
    );

    press(&mut app, KeyCode::KeyL, Some("l"));
    app.update();
    assert_eq!(feed(&app).level, ServiceLogLevelFilter::Errors);
    press(&mut app, KeyCode::KeyT, Some("t"));
    app.update();
    assert_eq!(feed(&app).time, ServiceLogTimeFilter::LastHour);

    press(&mut app, KeyCode::Escape, None);
    app.update();
    assert!(
        shell_of(&app).service_log.is_none(),
        "Esc closes the open stream"
    );

    assert_eq!(
        signals(&app).log_repaints,
        6,
        "each panel chord publishes exactly one repaint"
    );
    assert!(
        signals(&app).applied > applied_before,
        "the panel chords mark the frame applied"
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Arm 0b — while the panel owns the page, table motion (arm 3a) is blocked
/// but the tail shared-key arm still runs, and the first Escape belongs to the
/// panel, not to the feedback-dismissal arm 0c.
#[test]
fn arm_0b_panel_blocks_table_motion_and_closes_before_the_feedback_notice() {
    let mut shell = services_shell();
    shell.report_notice(
        taskmanager_shell::FeedbackSource::Interaction,
        taskmanager_shell::FeedbackSeverity::Info,
        taskmanager_shell::FeedbackLifecycle::UntilReplaced,
        "notice across the log panel",
    );
    let mut app = routed_app(shell, Page::Services);
    let target = sorted_service_ids(&app)[0].clone();
    app.world_mut().resource_mut::<ServiceSelection>().target = Some(ServiceId::new(&target));
    open_log(&mut app, &target);

    // ArrowDown is the table-motion key, but arm 3a needs a free surface, so
    // the page row stays put; the additive tail arm (4) still moves the
    // shell's own cursor.
    press(&mut app, KeyCode::ArrowDown, None);
    app.update();
    assert_eq!(
        page_target(&app),
        Some(target.clone()),
        "the panel owner blocks table motion"
    );
    assert_eq!(
        shell_of(&app).selected,
        1,
        "the tail shared-key arm still moves the shell cursor"
    );

    press(&mut app, KeyCode::Escape, None);
    app.update();
    assert!(shell_of(&app).service_log.is_none(), "Esc closes the panel");
    assert!(
        shell_of(&app).feedback_notice().is_some(),
        "the panel consumed the first Esc (0b before 0c)"
    );

    press(&mut app, KeyCode::Escape, None);
    app.update();
    assert!(
        shell_of(&app).feedback_notice().is_none(),
        "the freed keyboard clears the notice"
    );
}

/// Arm 0b — its chords need the Services page and an unmodified key, so
/// off-page presses keep their navigation meaning and chorded keys keep
/// reaching the shell's own routers.
#[test]
fn arm_0b_declines_off_page_and_for_chorded_keys() {
    let mut app = routed_app(services_shell(), Page::Processes);
    let target = sorted_service_ids(&app)[0].clone();
    open_log(&mut app, &target);

    press(&mut app, KeyCode::KeyP, Some("p"));
    app.update();
    assert_eq!(
        app.world().resource::<Route>().page,
        Page::Settings,
        "off the Services page bare P keeps its navigation meaning"
    );
    assert!(!feed(&app).paused, "the panel did not pause off-page");

    route_to(&mut app, Page::Services);
    app.update();

    press_ctrl(&mut app, KeyCode::KeyP, None);
    assert!(!feed(&app).paused, "Ctrl+P is not a panel chord");
    press_ctrl(&mut app, KeyCode::KeyF, None);
    assert!(
        shell_of(&app).search_active(),
        "Ctrl+F still reaches the shell from the panel page"
    );
    assert_eq!(
        app.world().resource::<Route>().page,
        Page::Services,
        "search owns the keyboard, so the route stays"
    );
}

/// Arm 2b — the Services open attempt (fallback to the first sorted row, the
/// 'a' chord, cancel, and the honest decline of a stale page selection).
#[test]
fn arm_2b_enter_opens_the_services_menu_from_the_first_sorted_row() {
    let mut app = routed_app(services_shell(), Page::Services);
    let ids = sorted_service_ids(&app);
    let applied_before = signals(&app).applied;

    press(&mut app, KeyCode::Enter, None);
    app.update();
    assert_eq!(
        service_menu_id(&app),
        Some(ids[0].clone()),
        "no page selection opens the first sorted row"
    );
    assert_eq!(
        signals(&app).applied,
        applied_before + 1,
        "the open marks the frame applied"
    );

    // The open menu owns every key: Esc cancels it (arm 0a).
    press(&mut app, KeyCode::Escape, None);
    app.update();
    assert_eq!(service_menu_id(&app), None, "Esc cancels the open menu");

    // A page selection the inventory no longer holds declines honestly: the
    // arm must not silently retarget the open attempt to the first row.
    app.world_mut().resource_mut::<ServiceSelection>().target = Some(ServiceId::new("svc-gone"));
    let applied_after_cancel = signals(&app).applied;
    press(&mut app, KeyCode::Enter, None);
    app.update();
    assert_eq!(service_menu_id(&app), None, "a stale target opens nothing");
    assert_eq!(
        signals(&app).applied,
        applied_after_cancel,
        "the declined press is never applied"
    );

    // The same arm carries the TUI's 'a' chord, and a live page selection
    // wins over the sorted first row.
    app.world_mut().resource_mut::<ServiceSelection>().target = Some(ServiceId::new(&ids[1]));
    press(&mut app, KeyCode::KeyA, Some("a"));
    app.update();
    assert_eq!(
        service_menu_id(&app),
        Some(ids[1].clone()),
        "the selected row wins on the 'a' chord"
    );
}

/// Arm 2b — the Startup and Sessions open attempts use their own page
/// selection and their own frozen row types.
#[test]
fn arm_2b_enter_opens_the_startup_and_sessions_menus_for_their_selected_rows() {
    let mut app = routed_app(fixture::demo_app(), Page::Startup);
    let entries = sorted_startup_ids(&app);
    app.world_mut().resource_mut::<StartupSelection>().target =
        Some(StartupEntryId::new(&entries[1]));
    press(&mut app, KeyCode::Enter, None);
    app.update();
    assert_eq!(
        startup_menu_id(&app),
        Some(entries[1].clone()),
        "Startup Enter uses the selected row"
    );

    press(&mut app, KeyCode::Escape, None);
    app.update();
    route_to(&mut app, Page::Sessions);
    app.update();
    app.update();
    let sessions = sorted_session_ids(&app);
    app.world_mut().resource_mut::<SessionSelection>().target = Some(SessionId::new(&sessions[0]));
    press(&mut app, KeyCode::KeyA, Some("a"));
    app.update();
    assert_eq!(
        session_menu_id(&app),
        Some(sessions[0].clone()),
        "Sessions 'a' uses the selected row"
    );
}

/// Arm 2b — the Processes page is deliberately excluded: without a process
/// selection the local 'a' chord declines rather than opening an inventory menu
/// (that page's action-chord arm owns the key instead), and no frame is marked
/// applied.
#[test]
fn arm_2b_excludes_the_processes_page() {
    let mut shell = ShellApp::new();
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
    let mut app = routed_app(shell, Page::Processes);

    press(&mut app, KeyCode::KeyA, Some("a"));
    app.update();
    assert_eq!(
        signals(&app).applied,
        0,
        "the declined press is not applied"
    );
    assert_eq!(
        service_menu_id(&app),
        None,
        "no Services menu on the process page"
    );
    assert_eq!(
        startup_menu_id(&app),
        None,
        "no Startup menu on the process page"
    );
    assert_eq!(
        session_menu_id(&app),
        None,
        "no Sessions menu on the process page"
    );
}

/// Arm 2b — no open attempt without a row: an inventory that reports nothing
/// declines honestly instead of opening an empty-verb menu.
#[test]
fn arm_2b_declines_without_an_inventory_row() {
    let mut shell = ShellApp::new();
    let _ = shell.apply_action(AppAction::SelectPage(AppPage::Services));
    let mut app = routed_app(shell, Page::Services);

    press(&mut app, KeyCode::Enter, None);
    app.update();
    assert_eq!(
        service_menu_id(&app),
        None,
        "an empty inventory opens nothing"
    );
    assert_eq!(
        signals(&app).applied,
        0,
        "the declined press is not applied"
    );
}

/// Arm 2 outranks arm 2b — an armed gate owns Enter, so the inventory open
/// attempt never runs and the gate's own confirm path submits the effect.
#[test]
fn arm_2_confirms_the_armed_service_gate_so_the_inventory_enter_arm_never_runs() {
    let mut shell = services_shell();
    let service = shell
        .sorted_services()
        .into_iter()
        .next()
        .cloned()
        .expect("fixture service");
    assert!(
        shell.select_service_control(&service, ServiceAction::Stop),
        "the fixture row carries a provider target"
    );
    let _ = shell.apply_action(AppAction::RequestServiceControl);
    assert_eq!(
        shell.confirmation_kind(),
        Some(ConfirmationKind::ServiceControl)
    );

    let mut app = routed_app(shell, Page::Services);
    press(&mut app, KeyCode::Enter, None);
    app.update();
    assert_eq!(
        shell_of(&app).confirmation_kind(),
        None,
        "Enter confirmed the armed gate"
    );
    assert_eq!(
        service_menu_id(&app),
        None,
        "the gate owns Enter, so the open attempt never runs"
    );
    assert!(
        app.world()
            .resource::<PendingEffects>()
            .0
            .iter()
            .any(|effect| matches!(effect, PlatformEffect::ServiceControl(_))),
        "the confirm submits the frozen service control request"
    );
}

/// Arm 2c — 't' on Performance arms the SMART gate for the focused disk, else
/// the first reported one; the gate's confirm path submits the frozen intent.
#[test]
fn arm_2c_key_t_arms_the_self_test_for_the_focused_disk_then_the_first_reported_one() {
    let mut shell = fixture::demo_app();
    let first = shell
        .projection()
        .snapshot
        .as_ref()
        .expect("demo snapshot")
        .disks[0]
        .device_id
        .clone();
    let second = "disk:demo:nvme1".to_owned();
    let extra = disk_row(&second, "nvme1n1", "Extra NVMe");
    fixture::edit_snapshot(&mut shell, |snapshot| {
        if let Some(snapshot) = snapshot {
            snapshot.disks.push(extra);
        }
    });

    let mut app = routed_app(shell, Page::Performance);
    app.insert_resource(PerformanceDeviceFocus(PerformanceDeviceTarget::Disk(
        second.clone(),
    )));

    // The text is the SHIFTED form, which the shell's char router would use to
    // toggle the suggestions overlay: arm 2c must consume the key first.
    press(&mut app, KeyCode::KeyT, Some("T"));
    app.update();
    assert!(
        !shell_of(&app).suggestions_open(),
        "the SMART arm consumed the key before the shell char router"
    );
    assert_eq!(
        smart_target(&app),
        Some(second.clone()),
        "the focused disk wins over the first reported row"
    );

    press(&mut app, KeyCode::Enter, None);
    app.update();
    assert_eq!(
        shell_of(&app).confirmation_kind(),
        None,
        "Enter confirms the SMART gate"
    );
    let submitted = app
        .world()
        .resource::<PendingEffects>()
        .0
        .iter()
        .any(|effect| {
            matches!(
                effect,
                PlatformEffect::SmartControl(SmartControlRequest::StartSelfTest(intent))
                    if intent.device_id.as_str() == second
                        && intent.kind == SmartSelfTestKind::Short
            )
        });
    assert!(
        submitted,
        "the confirm submits the focused disk's frozen short self-test"
    );

    // A non-disk focus falls back to the first reported disk.
    app.world_mut().resource_mut::<PerformanceDeviceFocus>().0 = PerformanceDeviceTarget::Cpu;
    press(&mut app, KeyCode::KeyT, Some("t"));
    app.update();
    assert_eq!(
        smart_target(&app),
        Some(first),
        "a non-disk focus falls back to the first reported disk"
    );
}

/// Arm 2c — its trigger conditions: at least one reported disk, and no armed
/// gate (an armed gate owns the keyboard, so 't' can never retarget it).
#[test]
fn arm_2c_needs_a_reported_disk_and_never_re_arms_over_an_armed_gate() {
    let mut app = routed_app(ShellApp::new(), Page::Performance);
    install_disks(&mut app, Vec::new());

    press(&mut app, KeyCode::KeyT, Some("t"));
    app.update();
    assert_eq!(
        shell_of(&app).confirmation_kind(),
        None,
        "no reported disk arms nothing"
    );
    assert_eq!(
        signals(&app).applied,
        0,
        "the declined arm leaves the frame unapplied"
    );

    install_disks(&mut app, vec![disk_row("disk:alpha", "nvme0n1", "Alpha")]);
    press(&mut app, KeyCode::KeyT, Some("t"));
    app.update();
    assert_eq!(
        smart_target(&app),
        Some("disk:alpha".to_owned()),
        "the first reported disk is the fallback target"
    );

    // The armed gate now owns the keyboard: a second 't' must keep the frozen
    // intent instead of re-arming or retargeting it.
    press(&mut app, KeyCode::KeyT, Some("t"));
    app.update();
    assert_eq!(
        smart_target(&app),
        Some("disk:alpha".to_owned()),
        "the frozen intent survives"
    );
    assert_eq!(
        shell_of(&app).confirmation_kind(),
        Some(ConfirmationKind::SmartSelfTest)
    );
}

/// Arm 3a — the additive table-motion arm and its position in the chain: the
/// page row moves and the tail shared-key arm runs in the same pass, with the
/// page row clamped at the table edges.
#[test]
fn arm_3a_arrows_move_the_services_row_and_the_tail_shared_arm_moves_the_shell_cursor() {
    let mut app = routed_app(services_shell(), Page::Services);
    let ids = sorted_service_ids(&app);
    app.world_mut().resource_mut::<ServiceSelection>().target = Some(ServiceId::new(&ids[0]));
    app.world_mut()
        .non_send_mut::<FrontendTrack>()
        .shell
        .selected = 0;

    press(&mut app, KeyCode::ArrowDown, None);
    app.update();
    assert_eq!(
        page_target(&app),
        Some(ids[1].clone()),
        "3a moves the page row"
    );
    assert_eq!(
        shell_of(&app).selected,
        1,
        "the tail shared-key arm ran too: the additive arms both fire"
    );

    press(&mut app, KeyCode::ArrowUp, None);
    app.update();
    assert_eq!(page_target(&app), Some(ids[0].clone()));
    assert_eq!(shell_of(&app).selected, 0);

    press(&mut app, KeyCode::ArrowUp, None);
    app.update();
    assert_eq!(
        page_target(&app),
        Some(ids[0].clone()),
        "the first row clamps"
    );
    assert_eq!(
        shell_of(&app).selected,
        0,
        "the shell cursor clamps with it"
    );
}

/// Arm 3a — Startup and Sessions carry the same arrow motion through their own
/// typed selection resources.
#[test]
fn arm_3a_arrows_move_the_startup_and_sessions_rows() {
    let mut app = routed_app(fixture::demo_app(), Page::Startup);
    let entries = sorted_startup_ids(&app);
    app.world_mut().resource_mut::<StartupSelection>().target =
        Some(StartupEntryId::new(&entries[0]));
    press(&mut app, KeyCode::ArrowDown, None);
    app.update();
    assert_eq!(
        app.world()
            .resource::<StartupSelection>()
            .target
            .as_ref()
            .map(StartupEntryId::as_str),
        Some(entries[1].as_str()),
        "Startup ArrowDown moves to the next entry"
    );

    route_to(&mut app, Page::Sessions);
    app.update();
    app.update();
    let sessions = sorted_session_ids(&app);
    app.world_mut().resource_mut::<SessionSelection>().target = Some(SessionId::new(&sessions[0]));
    press(&mut app, KeyCode::ArrowDown, None);
    app.update();
    assert_eq!(
        app.world()
            .resource::<SessionSelection>()
            .target
            .as_ref()
            .map(SessionId::as_str),
        Some(sessions[1].as_str()),
        "Sessions ArrowDown moves to the next session"
    );
}

/// Arm 3a — an armed confirmation gate owns the keyboard first, so table
/// motion only resumes once the gate is dismissed.
#[test]
fn a_confirmation_gate_blocks_arm_3a_until_it_is_dismissed() {
    let mut shell = services_shell();
    let service = shell
        .sorted_services()
        .into_iter()
        .next()
        .cloned()
        .expect("fixture service");
    assert!(shell.select_service_control(&service, ServiceAction::Restart));
    let _ = shell.apply_action(AppAction::RequestServiceControl);

    let mut app = routed_app(shell, Page::Services);
    let ids = sorted_service_ids(&app);
    app.world_mut().resource_mut::<ServiceSelection>().target = Some(ServiceId::new(&ids[0]));

    press(&mut app, KeyCode::ArrowDown, None);
    app.update();
    assert_eq!(
        page_target(&app),
        Some(ids[0].clone()),
        "the armed gate blocks table motion"
    );
    assert_eq!(
        shell_of(&app).confirmation_kind(),
        Some(ConfirmationKind::ServiceControl),
        "the gate stays armed"
    );

    press(&mut app, KeyCode::Escape, None);
    app.update();
    assert_eq!(
        shell_of(&app).confirmation_kind(),
        None,
        "Escape dismisses the gate"
    );

    press(&mut app, KeyCode::ArrowDown, None);
    app.update();
    assert_eq!(
        page_target(&app),
        Some(ids[1].clone()),
        "arm 3a resumes once the gate is gone"
    );
}
