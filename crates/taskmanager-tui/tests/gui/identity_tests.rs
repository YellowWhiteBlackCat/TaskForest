//! Stable identity anchors at the two remaining gaps (TUI-002 closeout):
//!
//! 1. the Performance page's device selection — a device family whose backing
//!    projection facts disappear (hot-unplug, provider going dark) must fall
//!    back fail-closed to a still-backed resource, never keep a stale
//!    selection into the next paint;
//! 2. cross-page round trips — the shell owns ONE cursor index for every
//!    table page, so leaving page A, letting batches reorder or shrink its
//!    rows, and returning must restore A's selected row by identity, with the
//!    deterministic clamped cursor as the vanished-identity fallback.
//!
//! Every fixture here is typed and deterministic (`taskmanager-shell::fixture`
//! plus the shared process builder); no host-derived value appears in any
//! assertion.

use super::*;
use taskmanager_application::{
    CorrelatedEvent, CorrelatedServiceEvent, KeyCode, Modifiers, PlatformEventContext,
    ProcessEvent, ServiceEvent,
};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::{
    CapabilityId, EventSequence, PartialSourceSnapshot, RequestId,
};
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture::{self, ProjectionSeedFact};

// ── shared fixtures ──────────────────────────────────────────────────────────

fn service(id: &str, name: &str) -> ServiceItem {
    ServiceItem::from_inventory(
        ServiceId::new(id),
        name,
        ServiceStatus::Active,
        "",
        "",
        "",
        "",
    )
}

/// Replace the Services projection through the same correlated provider
/// snapshot event a live refresh folds through the shell.
fn services_snapshot_batch(ids: &[&str]) -> PlatformEventBatch {
    let mut batch = PlatformEventBatch::default();
    batch.service_events.push(CorrelatedServiceEvent {
        request_id: RequestId::MIN,
        capability: CapabilityId::SERVICES,
        provider: None,
        sequence: EventSequence::new(1),
        observed_at_ms: 1,
        event: ServiceEvent::Snapshot(PartialSourceSnapshot::new(
            ids.iter()
                .map(|id| service(id, id.strip_prefix("service:").unwrap_or(id)))
                .collect(),
            Vec::new(),
        )),
    });
    batch
}

/// Replace the process projection through the same correlated provider
/// snapshot event a live refresh folds through the shell.
fn processes_snapshot_batch(processes: Vec<ProcessItem>) -> PlatformEventBatch {
    let mut batch = PlatformEventBatch::default();
    batch.process_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id: RequestId::MIN,
            capability: CapabilityId::PROCESS_LIST,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 1,
        },
        ProcessEvent::Snapshot(std::sync::Arc::new(processes)),
    ));
    batch
}

fn process(pid: u32, name: &str, cpu: f32) -> ProcessItem {
    let mut process = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.to_owned())
        .build();
    process.apply_scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid), 1),
        cpu_percentage: ScalarObservation::available(cpu, 1),
        ..Default::default()
    });
    process
}

/// The pid an Applications visual row resolves to, if any (a category header
/// has none).
fn row_pid(app: &TuiApp, index: usize) -> Option<u32> {
    let rows = app.process_rows_snapshot();
    match rows.get(index)? {
        crate::process_view::ProcessRow::TreeNode { process, .. } => Some(process.pid),
        crate::process_view::ProcessRow::Group {
            row_key: Some(taskmanager_shell::ProcessRowId::Application(identity)),
            ..
        } => Some(identity.pid()),
        crate::process_view::ProcessRow::Group { .. } => None,
    }
}

fn position_of_pid(app: &TuiApp, pid: u32) -> usize {
    (0..app.process_rows_snapshot().len())
        .find(|&index| row_pid(app, index) == Some(pid))
        .unwrap_or_else(|| panic!("fixture must render a visual row for pid {pid}"))
}

fn seed_applications(app: &mut TuiApp, processes: Vec<ProcessItem>) {
    fixture::seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Processes(Some(processes)),
    );
    app.application.active_page = AppPage::Applications;
    app.expanded_groups = ["category:uncategorized".to_string()].into_iter().collect();
}

fn selected_service_id(app: &TuiApp) -> String {
    app.sorted_service_at(app.selected)
        .expect("cursor inside the sorted services table")
        .id
        .as_str()
        .to_owned()
}

// ── device selection: fail-closed hot-unplug reconcile ───────────────────────

#[test]
fn gpu_hot_unplug_falls_back_to_the_first_still_backed_resource() {
    let mut app = TuiApp::from_shell(taskmanager_shell::fixture::demo_app());
    app.select_perf_device(PerfDevice::Gpu);
    assert!(app.visible_perf_devices().contains(&PerfDevice::Gpu));
    app.gpu_engine_scroll = 5;

    // The GPU family leaves the projection (hot-unplug); the next wave folds
    // through the TUI's production batch entry.
    fixture::edit_snapshot(&mut app.shell, |snapshot| {
        if let Some(snapshot) = snapshot.as_mut() {
            snapshot.gpu.clear();
        }
    });
    app.apply_platform_batch(PlatformEventBatch::default());

    assert!(
        !app.visible_perf_devices().contains(&PerfDevice::Gpu),
        "fixture must have removed the GPU family from the projection"
    );
    assert_eq!(
        app.perf_device,
        PerfDevice::Cpu,
        "the fallback must be the first resource the projection still backs"
    );
    assert!(
        app.visible_perf_devices().contains(&app.perf_device),
        "the reconciled selection must be backed by real facts"
    );
    assert_eq!(
        app.gpu_engine_scroll, 0,
        "the fallback must not inherit the vanished device's viewport intent"
    );
}

#[test]
fn an_unrelated_batch_never_moves_a_backed_device_selection() {
    let mut app = TuiApp::from_shell(taskmanager_shell::fixture::demo_app());
    app.select_perf_device(PerfDevice::Gpu);
    app.apply_platform_batch(PlatformEventBatch::default());
    assert_eq!(app.perf_device, PerfDevice::Gpu);
}

#[test]
fn with_no_visible_resource_the_explicit_empty_state_is_kept() {
    let mut app = TuiApp::from_shell(taskmanager_shell::fixture::demo_app());
    app.select_perf_device(PerfDevice::Gpu);
    // Preference-gated families hidden and no facts left to back the rest:
    // nothing is selectable, so there is no honest fallback target.
    app.prefs.show = [false; 10];
    app.apply_platform_batch(PlatformEventBatch::default());

    assert!(app.visible_perf_devices().is_empty());
    assert_eq!(
        app.perf_device,
        PerfDevice::Gpu,
        "with no backed resource, the raw token is kept as the empty state; \
         the panels render honest absence and nothing is fabricated"
    );
}

// ── cross-page round trips: flat inventory pages ─────────────────────────────

#[test]
fn page_key_round_trip_restores_the_services_row_identity_after_an_offpage_reorder() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    fixture::seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Services(Some(vec![
            service("service:zeta", "zeta"),
            service("service:alpha", "alpha"),
        ])),
    );
    // The production key path: Alt+3 opens Services, Alt+1 returns to
    // Performance — both through the TUI's key wrapper hygiene.
    let _ = app.handle_local_key(ShellKeyEvent::new(KeyCode::Digit3, Modifiers::ALT));
    assert_eq!(app.page(), AppPage::Services);
    assert_eq!(selected_service_id(&app), "service:zeta");

    let _ = app.handle_local_key(ShellKeyEvent::new(KeyCode::Digit1, Modifiers::ALT));
    assert_eq!(app.page(), AppPage::Performance);
    // While Services is hidden, its provider snapshot reorders the rows; the
    // shell clamps the shared cursor, so the raw index no longer points at
    // the selected row.
    app.apply_platform_batch(services_snapshot_batch(&["service:alpha", "service:zeta"]));

    let _ = app.handle_local_key(ShellKeyEvent::new(KeyCode::Digit3, Modifiers::ALT));
    assert_eq!(app.page(), AppPage::Services);
    assert_eq!(
        selected_service_id(&app),
        "service:zeta",
        "the round trip must restore the row by provider identity, not by the \
         shared cursor index that other pages and batches moved"
    );
    assert_eq!(app.selected, 1, "zeta moved to index one in the new order");
}

/// The selected row's provider identity for one flat inventory page.
type PageIdentityFn = fn(&TuiApp) -> Option<String>;

fn services_identity(app: &TuiApp) -> Option<String> {
    app.sorted_service_at(app.selected)
        .map(|row| row.id.as_str().to_owned())
}

fn startup_identity(app: &TuiApp) -> Option<String> {
    app.sorted_startup_entry_at(app.selected)
        .map(|row| row.id.as_str().to_owned())
}

fn session_identity(app: &TuiApp) -> Option<String> {
    app.sorted_session_at(app.selected)
        .map(|row| row.id.to_string())
}

#[test]
fn inventory_round_trip_restores_each_table_page_identity() {
    let cases: [(AppPage, PageIdentityFn); 3] = [
        (AppPage::Services, services_identity),
        (AppPage::Startup, startup_identity),
        (AppPage::Users, session_identity),
    ];
    for (page, identity_of) in cases {
        let mut app = crate::demo_app();
        let _ = app.apply_action(AppAction::SelectPage(page));
        let identity = identity_of(&app)
            .filter(|id| !id.is_empty())
            .expect("fixture row with a provider identity");

        let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
        let _ = app.apply_action(AppAction::SelectPage(page));

        assert_eq!(
            identity_of(&app),
            Some(identity),
            "{page:?} round trip must land on the identity captured at leave"
        );
    }
}

#[test]
fn services_round_trip_with_a_vanished_identity_falls_back_deterministically() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    fixture::seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Services(Some(vec![
            service("service:zeta", "zeta"),
            service("service:alpha", "alpha"),
        ])),
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));
    app.selected = app
        .sorted_services()
        .iter()
        .position(|row| row.id.as_str() == "service:alpha")
        .expect("fixture row");
    assert_eq!(app.selected, 1);

    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    app.apply_platform_batch(services_snapshot_batch(&["service:zeta"]));
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Services));

    assert_eq!(
        app.selected, 0,
        "the vanished identity falls back to the cursor clamped inside the \
         shrunk table"
    );
    assert_eq!(selected_service_id(&app), "service:zeta");
}

// ── cross-page round trips: the Applications category tree ───────────────────

#[test]
fn applications_round_trip_restores_the_same_process_row_after_an_offpage_refresh() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    seed_applications(
        &mut app,
        vec![process(1, "low", 10.0), process(2, "target", 90.0)],
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.selected = position_of_pid(&app, 2);
    app.sync_grouped_application_selection();
    let initial_index = app.selected;
    assert_eq!(row_pid(&app, initial_index), Some(2), "fixture cursor");

    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    // While hidden, the process domain refreshes and the target's CPU drops
    // below the other process, so its visual row index moves under the
    // default highest-CPU-first sort.
    app.apply_platform_batch(processes_snapshot_batch(vec![
        process(1, "low", 10.0),
        process(2, "target", 1.0),
    ]));

    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert_eq!(
        row_pid(&app, app.selected),
        Some(2),
        "the round trip must follow the process identity (pid + provider \
         start-token) to its new visual row"
    );
    assert_ne!(
        app.selected, initial_index,
        "the fixture must actually move the row, or this test proves nothing"
    );
}

#[test]
fn applications_round_trip_with_an_exited_process_falls_back_in_bounds() {
    let mut app = TuiApp::from_shell(ShellApp::new());
    seed_applications(
        &mut app,
        vec![
            process(1, "low", 10.0),
            process(2, "high", 90.0),
            process(3, "gone", 50.0),
        ],
    );
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app.selected = position_of_pid(&app, 3);
    app.sync_grouped_application_selection();
    assert_eq!(row_pid(&app, app.selected), Some(3), "fixture cursor");

    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    // pid 3 exits while the page is hidden; the projection shrinks to pid 1.
    app.apply_platform_batch(processes_snapshot_batch(vec![process(1, "low", 10.0)]));

    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert_eq!(
        row_pid(&app, app.selected),
        Some(1),
        "the exited process must not be faked back; the cursor lands on a \
         live row instead"
    );
    assert!(
        app.selected < app.process_rows_snapshot().len(),
        "the fallback cursor stays inside the shrunk visual list"
    );
}
