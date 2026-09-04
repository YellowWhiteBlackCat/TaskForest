//! Selected-service details column (GPUI `services_view/details.rs` parity).
//!
//! The panel must render the selected service's state triplet from the
//! inventory row, its four read-only relation rows from the shell's canonical
//! `ServiceDependenciesLifecycle`, follow the table's `selected` cursor with
//! one shared source, and yield the whole column back to the table on frames
//! that cannot afford it. Every assertion reads painted frame text — never
//! source text, never host values.

use super::frame_text;

use taskmanager_application::{AppPage, i18n};
use taskmanager_core::core::services::{
    ServiceDeps, ServiceItem, ServiceRelationKind, ServiceStatus,
};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::RequestId;

/// The reference frame the details column is designed for: wide and tall
/// enough to afford the panel beside the table.
const WIDE_WIDTH: u16 = 120;
const WIDE_HEIGHT: u16 = 36;

fn service(id: &str, load: &str, active: &str, sub: &str) -> ServiceItem {
    ServiceItem::from_inventory(
        ServiceId::new(id.to_owned()),
        id,
        ServiceStatus::Active,
        "Fixture service",
        load,
        active,
        sub,
    )
}

/// Two services with distinguishable triplets: alpha runs, beta exited. The
/// fixture replaces the demo inventory through the shell-owned seed reducer,
/// so the canonical store and its revision stay the only mutated facts.
fn seed_two_services(app: &mut crate::TuiApp) -> (ServiceId, ServiceId) {
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Services(Some(vec![
            service("alpha.service", "loaded", "active", "running"),
            service("beta.service", "loaded", "inactive", "exited"),
        ])),
    );
    (
        ServiceId::new("alpha.service"),
        ServiceId::new("beta.service"),
    )
}

/// Drive the shared dependency lifecycle through the same typed
/// attempt → request → resolve correlation a platform completion takes, so
/// the fixture fact reaches the panel exactly the way a real answer does.
fn resolve_relations(app: &mut crate::TuiApp, target: ServiceId, dependencies: ServiceDeps) {
    let attempt = app.shell.service_dependencies.begin_attempt(target.clone());
    let request_id = RequestId::new(7).expect("nonzero request id");
    assert!(
        app.shell
            .service_dependencies
            .accept_attempt(attempt, request_id)
    );
    assert!(
        app.shell
            .service_dependencies
            .resolve(request_id, target, dependencies)
    );
}

#[test]
fn the_selected_service_renders_its_state_triplet() {
    let mut app = crate::TuiApp::demo();
    app.application.active_page = AppPage::Services;
    seed_two_services(&mut app);
    app.selected = 0;

    let frame = frame_text(&app, WIDE_WIDTH, WIDE_HEIGHT);
    assert!(frame.contains("Load state"), "the triplet labels paint");
    assert!(frame.contains("loaded"), "load_state paints from the row");
    assert!(
        frame.contains("running"),
        "the selected row's sub_state paints"
    );
    assert!(
        !frame.contains("exited"),
        "the unselected row's sub_state must not leak into the panel"
    );
    // The channel never opened (demo/Closed), so all four relation rows are
    // honest dashes — the closed channel reads as absence, not as an empty
    // success, and the loading note stays silent.
    let dashes = frame.matches('—').count();
    assert!(
        dashes >= 4,
        "four relation rows degrade to dashes, got {dashes}"
    );
    assert!(
        !frame.contains("Loading dependency details"),
        "a closed channel is not loading"
    );
}

#[test]
fn a_resolved_dependency_capture_paints_its_relation_targets() {
    let mut app = crate::TuiApp::demo();
    app.application.active_page = AppPage::Services;
    let (alpha, _beta) = seed_two_services(&mut app);
    app.selected = 0;

    let mut deps = ServiceDeps::default();
    deps.replace_relation_targets(
        ServiceRelationKind::Requires,
        [ServiceId::new("network.target")],
    );
    deps.replace_relation_targets(
        ServiceRelationKind::WantedBy,
        [ServiceId::new("multi-user.target")],
    );
    resolve_relations(&mut app, alpha, deps);

    let frame = frame_text(&app, WIDE_WIDTH, WIDE_HEIGHT);
    assert!(frame.contains("Requires"));
    assert!(frame.contains("Wanted by"));
    assert!(frame.contains("network.target"));
    assert!(frame.contains("multi-user.target"));
    // Untargeted kinds (Wants, After) stay honest dashes while targeted
    // kinds render their values.
    let dashes = frame.matches('—').count();
    assert!(
        dashes >= 2,
        "untargeted relation rows keep dashes, got {dashes}"
    );
}

#[test]
fn a_ready_channel_with_no_edges_paints_dashes_not_invented_targets() {
    let mut app = crate::TuiApp::demo();
    app.application.active_page = AppPage::Services;
    let (alpha, _beta) = seed_two_services(&mut app);
    app.selected = 0;
    resolve_relations(&mut app, alpha, ServiceDeps::default());

    let frame = frame_text(&app, WIDE_WIDTH, WIDE_HEIGHT);
    let dashes = frame.matches('—').count();
    assert!(
        dashes >= 4,
        "an empty canonical graph degrades every relation row to a dash, got {dashes}"
    );
}

#[test]
fn moving_the_table_selection_moves_the_details_with_it() {
    let mut app = crate::TuiApp::demo();
    app.application.active_page = AppPage::Services;
    seed_two_services(&mut app);

    app.selected = 0;
    let first = frame_text(&app, WIDE_WIDTH, WIDE_HEIGHT);
    assert!(
        first.contains("running") && !first.contains("exited"),
        "the panel shows row one's sub_state"
    );

    // The panel and the table read one `selected` cursor, so the cursor's
    // next position re-aims the whole panel at the next row.
    app.selected = 1;
    let second = frame_text(&app, WIDE_WIDTH, WIDE_HEIGHT);
    assert!(
        second.contains("exited") && !second.contains("running"),
        "the panel follows the cursor to row two's sub_state"
    );
}

#[test]
fn a_capture_aimed_at_another_service_never_leaks_into_the_selected_panel() {
    let mut app = crate::TuiApp::demo();
    app.application.active_page = AppPage::Services;
    let (alpha, _beta) = seed_two_services(&mut app);
    app.selected = 1;

    let mut deps = ServiceDeps::default();
    deps.replace_relation_targets(
        ServiceRelationKind::Requires,
        [ServiceId::new("network.target")],
    );
    resolve_relations(&mut app, alpha, deps);

    let frame = frame_text(&app, WIDE_WIDTH, WIDE_HEIGHT);
    assert!(
        !frame.contains("network.target"),
        "a stale capture for the previous row must not render against this row"
    );
    let dashes = frame.matches('—').count();
    assert!(
        dashes >= 4,
        "the mismatched channel reads as absence, got {dashes}"
    );
}

#[test]
fn a_narrow_frame_yields_the_whole_details_column_back_to_the_table() {
    let mut app = crate::TuiApp::demo();
    app.application.active_page = AppPage::Services;
    seed_two_services(&mut app);
    app.selected = 0;

    let frame = frame_text(&app, 80, WIDE_HEIGHT);
    assert!(
        !frame.contains("Load state"),
        "the panel drops out entirely instead of overlapping the table"
    );
    assert!(
        frame.contains("alpha.service"),
        "the table keeps painting its rows at the full width"
    );

    // One body row short of the panel's minimum height degrades the same
    // honest way while still painting the table rows it can.
    let short = frame_text(&app, WIDE_WIDTH, 16);
    assert!(
        !short.contains("Load state"),
        "a short frame degrades the same honest way"
    );
    assert!(short.contains("alpha.service"));
}

#[test]
fn the_details_column_labels_translate_across_both_locales() {
    let mut app = crate::TuiApp::demo();
    app.application.active_page = AppPage::Services;
    seed_two_services(&mut app);
    app.selected = 0;

    // frame_text pins English; the En side resolves the same key the frame
    // painted so a hardcoded string cannot satisfy only one side.
    let en = frame_text(&app, WIDE_WIDTH, WIDE_HEIGHT);
    let load_en = i18n::t("svc.load_state");
    assert_ne!(
        load_en, "svc.load_state",
        "the key exists in the en catalog"
    );
    assert!(en.contains(load_en));

    let (zh_frame, zh_pair) = super::acceptance_support::with_frame_in_language(
        &app,
        WIDE_WIDTH,
        WIDE_HEIGHT,
        i18n::Language::Zh,
        |frame| {
            let label = i18n::t("svc.load_state");
            let relation = i18n::t("svc.wanted_by");
            (frame.to_owned(), (label, relation))
        },
    );
    assert_ne!(zh_pair.0, load_en, "svc.load_state must translate");
    assert!(zh_frame.contains(zh_pair.0), "the triplet label translates");
    assert_ne!(
        zh_pair.1, "svc.wanted_by",
        "the key exists in the zh catalog"
    );
    assert!(
        zh_frame.contains(zh_pair.1),
        "the relation label translates"
    );
}

#[test]
fn service_dependencies_modal_opens_and_renders_relations() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(taskmanager_application::AppAction::SelectPage(
        taskmanager_application::AppPage::Services,
    ));
    assert!(
        app.open_service_dependencies(),
        "opens dependencies modal on services page"
    );
    assert_eq!(
        app.local_surface_kind(),
        Some(crate::TuiSurfaceKind::ServiceDependencies)
    );

    let service_id = app
        .sorted_service_at(app.selected)
        .expect("selected service")
        .id
        .clone();
    let deps = ServiceDeps::from_relations(
        taskmanager_core::core::services::ServiceRelationGraph::from_edges(vec![
            taskmanager_core::core::services::ServiceRelationEdge::new(
                taskmanager_core::core::services::ServiceRelationKind::Requires,
                "systemd-journald.service",
            ),
        ]),
    );
    resolve_relations(&mut app, service_id, deps);

    let frame = frame_text(&app, WIDE_WIDTH, WIDE_HEIGHT);
    assert!(
        frame.contains("systemd-journald.service"),
        "dependencies modal renders relation targets, got:
{frame}"
    );

    app.service_dependencies_scroll(2);
    let target = app
        .service_dependencies_mut()
        .expect("target still present");
    assert_eq!(target.scroll, 2, "modal scroll moves");

    app.close_local_overlays();
    assert_eq!(app.local_surface_kind(), None, "closes on dismiss");
}

#[test]
fn batch_menu_end_on_multi_select_arms_batch_confirmation_with_targets() {
    let mut app = crate::demo_app();
    let _ = app.apply_action(taskmanager_application::AppAction::SelectPage(
        taskmanager_application::AppPage::Applications,
    ));

    let processes = app.shell.projection().processes_slice().to_vec();
    assert!(processes.len() >= 2);
    let p0 = taskmanager_core::core::process::ProcessLiveKey::from_process(&processes[0]).unwrap();
    let p1 = taskmanager_core::core::process::ProcessLiveKey::from_process(&processes[1]).unwrap();
    app.shell.toggle_selected_identity(p0);
    app.shell.toggle_selected_identity(p1);
    assert_eq!(
        app.shell.selected_identities().len(),
        2,
        "two processes marked"
    );

    // Open batch menu
    assert!(app.open_batch_menu(), "open batch menu");

    // Select End action (index 0)
    let _ = app.batch_menu_select();

    let pending = app.shell.pending_batch().expect("batch confirmation armed");
    assert_eq!(
        pending.targets.len(),
        2,
        "both targets frozen in batch intent"
    );
    assert_eq!(
        pending.action,
        taskmanager_core::core::process::ProcessBatchAction::End
    );

    // Render frame while confirmation is open
    let frame = frame_text(&app, WIDE_WIDTH, WIDE_HEIGHT);
    for target in &pending.targets {
        assert!(
            frame.contains(&target.pid.to_string()) && frame.contains(&target.name),
            "batch confirmation popup must display target PID and name: {} ({}), got:
{frame}",
            target.name,
            target.pid
        );
    }

    // Confirm batch
    let effect = app.shell.confirm_process_batch();
    assert!(
        matches!(effect, Some(taskmanager_application::PlatformEffect::ExecuteBatch(intent)) if intent.targets.len() == 2),
        "confirming emits ExecuteBatch with 2 targets"
    );
}
