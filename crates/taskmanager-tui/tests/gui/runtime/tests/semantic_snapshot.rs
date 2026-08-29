//! Behavior tests for the TUI semantic-snapshot channel (defect #9).
//!
//! [`TuiApp::semantic_snapshot`] is a pure projection of shell + TUI-local
//! state into the toolkit-neutral contract tree. These tests drive real app
//! state (injected process rows, snapshot scalars, confirmation gates,
//! TUI-local modals) and assert the PUBLISHED SEMANTICS: row identity and
//! count, typed name/CPU/memory cell values with honest `Unavailable`,
//! cursor-and-marked selection, the observed-only CPU graph, revision
//! mirroring, the bounded row publication, and the modal dialog surfaces —
//! never source text.

use super::super::*;

use std::collections::HashSet;

use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, MemoryMetrics, MemoryScalarObservations, ScalarObservation,
    SystemSnapshot,
};
use taskmanager_core::core::process::{FrozenProcessIdentity, ProcessItem};
use taskmanager_shell::{SortCol, SortDir};
use taskmanager_ui_contract::{SemanticAction, SemanticLiveRegion, SemanticNodeId, SemanticRole};

use crate::{ProcessDetailsSection, ProcessPropertiesTarget};

/// The publication bound the GPUI/Iced frontends also apply.
const EXPECTED_MAX_PUBLISHED_ROWS: usize = 64;

/// An Applications-page app with a deterministic (Pid ascending) table order.
fn app_on_applications_page() -> TuiApp {
    let mut app = TuiApp::from_shell(ShellApp::new());
    app.process_sort = (SortCol::Pid, SortDir::Asc);
    app.application.active_page = AppPage::Applications;
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    app
}

/// One process row with explicitly typed scalar observations: `Some` values
/// are available observations, `None` is a typed permission-denied
/// unavailability (never a legacy fallback).
fn process_with(pid: u32, name: &str, cpu: Option<f32>, memory_bytes: Option<u64>) -> ProcessItem {
    let mut item = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.to_owned())
        .build();
    let mut observations = *item.scalar_observations();
    observations.cpu_percentage = match cpu {
        Some(value) => ScalarObservation::available(value, 1),
        None => ScalarObservation::unavailable(FailureKind::PermissionDenied),
    };
    observations.memory_bytes = match memory_bytes {
        Some(value) => ScalarObservation::available(value, 1),
        None => ScalarObservation::unavailable(FailureKind::PermissionDenied),
    };
    item.apply_scalar_observations(observations);
    item
}

fn row_nodes(
    snapshot: &taskmanager_ui_contract::SemanticSnapshot,
) -> Vec<&taskmanager_ui_contract::SemanticNode> {
    snapshot
        .nodes()
        .filter(|node| node.role() == SemanticRole::Row)
        .collect()
}

/// N injected processes publish exactly N rows whose name/CPU/memory cell
/// semantics match the typed inputs, the cursor row and the batch-marked row
/// are selected, the observed CPU scalar publishes the graph, the revision
/// mirrors the shell's refresh counter, and the live region announces the
/// visible row count.
#[test]
fn process_rows_carry_typed_name_cpu_memory_and_selection_semantics() {
    let mut app = app_on_applications_page();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            process_with(101, "alpha", Some(12.5), Some(25_000)),
            process_with(102, "bravo", Some(50.0), None),
            process_with(103, "gamma", None, Some(50_000)),
            process_with(104, "   ", None, None),
        ])),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(SystemSnapshot {
            cpu: CpuMetrics::from_observations(CpuScalarObservations {
                global_usage_pct: ScalarObservation::available(31.0, 1),
                ..Default::default()
            }),
            memory: MemoryMetrics::from_observations(
                MemoryScalarObservations {
                    total_bytes: ScalarObservation::available(100_000, 1),
                    ..Default::default()
                },
                Default::default(),
            ),
            ..SystemSnapshot::default()
        }))),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRefresh,
    );
    app.selected = 2;
    app.shell.selected_rows.insert(
        taskmanager_shell::ProcessRowIdentity::from_parts(
            103,
            taskmanager_test_support::fixture_start_token(103),
        )
        .expect("non-zero parts"),
    );
    app.shell.set_feedback_activity("");
    app.shell.clear_feedback_notice();

    let snapshot = app.semantic_snapshot().expect("semantic tree must build");

    assert_eq!(
        snapshot.revision(),
        app.projection().refresh_count,
        "the revision must mirror the shell's refresh counter"
    );

    let rows = row_nodes(&snapshot);
    assert_eq!(rows.len(), 4, "one published Row per injected process");

    // Typed cell semantics: name passthrough (whitespace-only is unnamed),
    // available CPU/memory as percentages of the typed observations.
    let cell = |suffix: &str| {
        snapshot
            .get(&SemanticNodeId::owned(suffix))
            .and_then(|node| node.value_text())
            .map(str::to_owned)
    };
    assert_eq!(cell("row:101:cell:name"), Some("alpha".into()));
    assert_eq!(cell("row:101:cell:cpu"), Some("12.5%".into()));
    assert_eq!(cell("row:101:cell:memory"), Some("25.0%".into()));
    assert_eq!(cell("row:102:cell:memory"), Some("Unavailable".into()));
    assert_eq!(cell("row:103:cell:cpu"), Some("Unavailable".into()));
    assert_eq!(cell("row:104:cell:name"), Some("Unnamed process".into()));

    // Selection semantics: the TUI cursor row and the batch-marked row are
    // selected; the other rows truthfully report false.
    let selected_of = |row: &str| {
        snapshot
            .get(&SemanticNodeId::owned(row))
            .and_then(|node| node.state().selected)
    };
    assert_eq!(selected_of("row:101"), Some(false));
    assert_eq!(selected_of("row:102"), Some(true), "cursor row selected");
    assert_eq!(selected_of("row:103"), Some(true), "marked row selected");
    assert_eq!(selected_of("row:104"), Some(false));

    // The observed CPU scalar publishes the graph with the typed current.
    let graph = snapshot
        .get(&SemanticNodeId::borrowed("cpu-graph"))
        .expect("observed CPU scalar must publish the graph");
    let numeric = graph.numeric_value().expect("graph numeric value");
    assert_eq!(numeric.current, 31.0);
    assert_eq!(numeric.maximum, 100.0);

    // The polite live region announces the visible row count.
    let status = snapshot
        .get(&SemanticNodeId::borrowed("status"))
        .expect("status live region present");
    assert_eq!(status.live_region(), SemanticLiveRegion::Polite);
    assert_eq!(status.name(), Some("4 processes visible"));
}

#[test]
fn semantic_process_rows_follow_the_visual_tree_visibility() {
    let mut app = app_on_applications_page();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            process_with(201, "parent", None, Some(1_000)),
            process_with(202, "child", Some(12.0), Some(2_000)),
        ])),
    );

    // The default category tree is expanded; semantic rows mirror both the
    // structural group rows and the two visible process nodes.
    let expanded = app.semantic_snapshot().expect("expanded semantic tree");
    assert_eq!(row_nodes(&expanded).len(), 2);
    assert!(
        expanded
            .nodes()
            .any(|node| node.role() == SemanticRole::TreeItem),
        "the visual category/application group must have a semantic TreeItem"
    );

    app.expanded_groups.clear();
    let collapsed = app.semantic_snapshot().expect("collapsed semantic tree");
    assert_eq!(
        row_nodes(&collapsed).len(),
        0,
        "collapsed visual process rows must not remain in semantics"
    );
    assert!(
        collapsed
            .nodes()
            .any(|node| node.role() == SemanticRole::TreeItem),
        "a collapsed visual group remains available to assistive technology"
    );
}

/// A first frame with no system snapshot omits the CPU graph entirely (an
/// unobserved scalar is not a measured 0%) and keeps unobserved row scalars
/// as `Unavailable` rather than zero.
#[test]
fn first_loading_frame_omits_unobserved_graph_and_keeps_scalars_unavailable() {
    let mut app = app_on_applications_page();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![process_with(
            77,
            "unobserved",
            None,
            None,
        )])),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRefresh,
    );

    let snapshot = app.semantic_snapshot().expect("loading tree must build");

    assert!(
        snapshot
            .get(&SemanticNodeId::borrowed("cpu-graph"))
            .is_none(),
        "an unobserved CPU scalar must not fabricate a 0% graph"
    );
    assert_eq!(row_nodes(&snapshot).len(), 1);
    let cell = |suffix: &str| {
        snapshot
            .get(&SemanticNodeId::owned(suffix))
            .and_then(|node| node.value_text())
    };
    assert_eq!(cell("row:77:cell:cpu"), Some("Unavailable"));
    assert_eq!(cell("row:77:cell:memory"), Some("Unavailable"));

    // A set footer status line is passed through to the live region verbatim
    // (the count fallback only applies to an empty status).
    app.shell.set_feedback_activity("Refreshing process list");
    let with_status = app.semantic_snapshot().expect("status tree must build");
    assert_eq!(
        with_status
            .get(&SemanticNodeId::borrowed("status"))
            .and_then(|node| node.name()),
        Some("Refreshing process list")
    );
}

/// An armed shell confirmation gate (end-task) publishes a dismissible modal
/// dialog whose description carries the frozen target identity.
#[test]
fn shell_end_task_confirmation_is_a_dismissible_dialog() {
    let mut app = app_on_applications_page();
    let target = FrozenProcessIdentity::from_authoritative_parts(1810, "worker", 7_500, 9_000)
        .expect("valid fixture identity");
    let _ = app.shell.application.interaction.reduce(
        taskmanager_application::InteractionEvent::ArmConfirmation(
            taskmanager_application::PendingConfirmation::EndTask(target),
        ),
    );

    let snapshot = app.semantic_snapshot().expect("modal tree must build");
    let modal = snapshot
        .get(&SemanticNodeId::owned("modal:end-task-confirmation"))
        .expect("end-task confirmation modal node");

    assert_eq!(modal.role(), SemanticRole::Dialog);
    assert!(modal.state().modal);
    assert!(modal.state().focusable);
    assert!(modal.supports_action(SemanticAction::Dismiss));
    let description = modal.description().unwrap_or_default();
    assert!(
        description.contains("1810") && description.contains("worker"),
        "the modal description must name the frozen target, got {description:?}"
    );
}

/// The TUI-local Process Properties modal publishes the same dialog
/// semantics, and dismissing the modal removes the dialog node again.
#[test]
fn tui_local_properties_modal_publishes_and_releases_the_dialog() {
    let mut app = app_on_applications_page();
    let item = process_with(4242, "editor", None, None);
    let identity = FrozenProcessIdentity::from_authoritative_parts(4242, "editor", 7_500, 9_000)
        .expect("fixture identity");
    app.process_properties_view = Some(ProcessPropertiesTarget {
        item,
        section: ProcessDetailsSection::Overview,
        scroll: 0,
    });
    assert!(app.shell.open_process_properties_for(identity));

    let open = app.semantic_snapshot().expect("properties tree must build");
    let modal = open
        .get(&SemanticNodeId::owned("modal:process-properties-modal"))
        .expect("properties modal node");
    assert_eq!(modal.role(), SemanticRole::Dialog);
    assert!(modal.state().modal);
    assert!(modal.supports_action(SemanticAction::Dismiss));
    let description = modal.description().unwrap_or_default();
    assert!(
        description.contains("4242") && description.contains("editor"),
        "the properties description must name its process, got {description:?}"
    );

    app.shell.dismiss_overlay();
    let closed = app.semantic_snapshot().expect("closed tree must build");
    assert!(
        closed
            .nodes()
            .all(|node| node.role() != SemanticRole::Dialog),
        "no dialog node may remain after every modal is closed"
    );
}

/// The published tree stays inside the toolkit-neutral semantic vocabulary
/// (the contract's types expose no geometry) and every row publishes exactly
/// its three domain cells — name, CPU, memory — with no layout detail.
#[test]
fn snapshot_stays_in_the_semantic_vocabulary_with_three_domain_cells_per_row() {
    let mut app = app_on_applications_page();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![
            process_with(201, "alpha", Some(1.0), Some(1_000)),
            process_with(202, "beta", None, None),
        ])),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRefresh,
    );

    let snapshot = app.semantic_snapshot().expect("semantic tree must build");

    let allowed: HashSet<SemanticRole> = [
        SemanticRole::Application,
        SemanticRole::Main,
        SemanticRole::Table,
        SemanticRole::ColumnHeader,
        SemanticRole::Row,
        SemanticRole::TreeItem,
        SemanticRole::Cell,
        SemanticRole::Graph,
        SemanticRole::StaticText,
    ]
    .into();
    for node in snapshot.nodes() {
        assert!(
            allowed.contains(&node.role()),
            "role {:?} is outside the toolkit-neutral vocabulary",
            node.role()
        );
    }

    for row in row_nodes(&snapshot) {
        let children: Vec<_> = row.children().collect();
        assert_eq!(
            children.len(),
            3,
            "a published row carries exactly name/cpu/memory cells"
        );
        for child in children {
            assert_eq!(
                snapshot.get(child).map(|node| node.role()),
                Some(SemanticRole::Cell)
            );
        }
    }
}

/// Row publication is bounded: a 70-process list publishes the first
/// EXPECTED_MAX_PUBLISHED_ROWS rows of the active ordering, not all 70.
#[test]
fn row_publication_is_bounded_to_the_reader_friendly_prefix() {
    let mut app = app_on_applications_page();
    let processes: Vec<ProcessItem> = (0..70)
        .map(|index| process_with(1000 + index, "proc", Some(0.5), Some(1_000)))
        .collect();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(processes)),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::AdvanceRefresh,
    );

    let snapshot = app.semantic_snapshot().expect("bounded tree must build");
    let interactive_rows = snapshot
        .nodes()
        .filter(|node| matches!(node.role(), SemanticRole::Row | SemanticRole::TreeItem))
        .count();
    assert_eq!(
        interactive_rows, EXPECTED_MAX_PUBLISHED_ROWS,
        "a 70-process visual-row list must publish only the bounded prefix"
    );
}
