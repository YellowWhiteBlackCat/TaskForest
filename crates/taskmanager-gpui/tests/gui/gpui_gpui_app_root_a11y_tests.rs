use super::{apply_accessibility_action, build_snapshot};
use crate::gpui_app::root::{ProcessDetailsSection, RootView, TopPage};
use gpui::{AppContext, Entity, TestAppContext};
use taskmanager_core::core::process::{ProcessItem, ProcessLiveKey, ProcessScalarObservations};
use taskmanager_core::core::{CpuScalarObservations, ScalarObservation};
use taskmanager_theme::Theme;
use taskmanager_ui_contract::{
    AccessibilityActionRejection, AccessibilityActionRequest, SemanticAction, SemanticNodeId,
    SemanticRole,
};

/// Build a bare RootView (no window needed — `build_snapshot` only reads
/// view state) inside a `#[gpui::test]` app context.
fn make_root(cx: &mut TestAppContext) -> Entity<RootView> {
    cx.update(|cx| cx.new(|cx| RootView::new(Theme::dark(), cx)))
}

fn snapshot_of(
    cx: &mut TestAppContext,
    root: &Entity<RootView>,
    revision: u64,
) -> taskmanager_ui_contract::SemanticSnapshot {
    root.read_with(cx, |view, _| build_snapshot(view, revision))
        .expect("canonical snapshot must build for view state")
}

fn process(pid: u32, name: &str) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.into())
        .scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(u64::from(pid) + 100, 1),
            ..ProcessScalarObservations::default()
        })
        .build()
}

fn identity(pid: u32) -> ProcessLiveKey {
    ProcessLiveKey::from_parts(pid, u64::from(pid) + 100).expect("fixture identity")
}

fn process_row_id(pid: u32) -> SemanticNodeId {
    SemanticNodeId::owned(format!(
        "row:process:pid:{pid}:start:{}",
        pid.saturating_add(100)
    ))
}

/// (a) A plain Apps page yields a well-formed tree: an Application root
/// with a Main landmark, a Table with column headers, one Row (with Cells)
/// per process, the CPU graph, and the status live region. All values are
/// read back without panicking.
#[gpui::test]
async fn apps_page_snapshot_has_expected_roles_and_values(cx: &mut TestAppContext) {
    let root = make_root(cx);
    root.update(cx, |view, _| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Apps;
        view.replace_processes_for_test(vec![process(1001, "alpha"), process(2002, "bravo")]);
        view.replace_process_selection([identity(2002)], None);
        // Make the graph assertion data-backed; RootView starts with an
        // unobserved snapshot and must not turn that state into 0%.
        view.replace_system_snapshot_for_test(taskmanager_core::core::metrics::SystemSnapshot {
            cpu: taskmanager_core::core::metrics::CpuMetrics::from_observations(
                CpuScalarObservations {
                    global_usage_pct: ScalarObservation::available(31.0, 1),
                    ..Default::default()
                },
            ),
            ..Default::default()
        });
    });

    let snapshot = snapshot_of(cx, &root, 7);
    assert_eq!(snapshot.revision(), 7);
    assert_eq!(snapshot.root().as_str(), "app");
    assert!(
        snapshot.nodes().count() > 0,
        "an Apps page must publish at least one node"
    );

    let root_node = snapshot
        .get(&SemanticNodeId::borrowed("app"))
        .expect("root node present");
    assert_eq!(root_node.role(), SemanticRole::Application);
    assert!(!root_node.children().next().is_none());

    let main = snapshot
        .get(&SemanticNodeId::borrowed("main"))
        .expect("main landmark present");
    assert_eq!(main.role(), SemanticRole::Main);

    let table = snapshot
        .get(&SemanticNodeId::borrowed("process-table"))
        .expect("process table present");
    assert_eq!(table.role(), SemanticRole::Table);

    let rows: Vec<_> = snapshot
        .nodes()
        .filter(|node| node.role() == SemanticRole::Row)
        .collect();
    assert_eq!(rows.len(), 2, "one published Row per process");

    // Data-driven cell values (injected process names / cpu / memory).
    let alpha_cell = snapshot
        .get(&SemanticNodeId::owned(format!(
            "{}:cell:name",
            process_row_id(1001).as_str()
        )))
        .expect("alpha name cell present");
    assert_eq!(alpha_cell.value_text(), Some("alpha"));
    let bravo_cpu_cell = snapshot
        .get(&SemanticNodeId::owned(format!(
            "{}:cell:cpu",
            process_row_id(2002).as_str()
        )))
        .expect("bravo cpu cell present");
    assert!(bravo_cpu_cell.value_text().is_some());
    let bravo_row = snapshot
        .get(&process_row_id(2002))
        .expect("bravo row present");
    assert_eq!(bravo_row.state().selected, Some(true));
    assert!(bravo_row.supports_action(SemanticAction::Select));

    // The CPU graph publishes a numeric value within range.
    let graph = snapshot
        .get(&SemanticNodeId::borrowed("cpu-graph"))
        .expect("cpu graph present");
    assert_eq!(graph.role(), SemanticRole::Graph);
    let numeric = graph
        .numeric_value()
        .expect("graph carries a numeric value");
    assert!(numeric.current >= 0.0 && numeric.current <= numeric.maximum);
    assert!(graph.supports_action(SemanticAction::ReadNextValue));

    // The polite live region announces the row count.
    let status = snapshot
        .get(&SemanticNodeId::borrowed("status"))
        .expect("status live region present");
    let announcement = status.name().unwrap_or_default();
    assert!(
        announcement.contains("2"),
        "status announcement must reflect the published row count, got {announcement:?}"
    );
}

/// (c) Edge-case values never break the snapshot: empty process list,
/// extreme CPU percentages (clamped), and an open modal dialog with
/// details state all still build a well-formed tree with sane roles.
#[gpui::test]
async fn snapshot_stays_well_formed_with_edge_values_and_modal_open(cx: &mut TestAppContext) {
    let root = make_root(cx);

    // Empty list: the tree still has its landmarks and a row-free table.
    let empty = snapshot_of(cx, &root, 1);
    assert!(empty.get(&SemanticNodeId::borrowed("main")).is_some());
    assert!(
        empty.get(&SemanticNodeId::borrowed("cpu-graph")).is_none(),
        "an unobserved first frame must not publish a fabricated CPU graph"
    );
    assert_eq!(
        empty
            .nodes()
            .filter(|node| node.role() == SemanticRole::Row)
            .count(),
        0
    );

    // Extreme CPU + a replacement transition (Run -> Properties) must not
    // panic and must keep the canonical tree shape.
    root.update(cx, |view, _| {
        view.replace_processes_for_test(vec![process(3, "hog"), process(4, "idle")]);
        view.replace_system_snapshot_for_test(taskmanager_core::core::metrics::SystemSnapshot {
            cpu: taskmanager_core::core::metrics::CpuMetrics::from_observations(
                CpuScalarObservations {
                    global_usage_pct: ScalarObservation::available(88.0, 1),
                    ..Default::default()
                },
            ),
            ..Default::default()
        });
        view.show_run_task();
        view.open_process_details(identity(3), ProcessDetailsSection::Overview);
        assert_eq!(view.process_properties_identity(), Some(identity(3)));
    });
    let busy = snapshot_of(cx, &root, 2);
    let graph = busy
        .get(&SemanticNodeId::borrowed("cpu-graph"))
        .expect("cpu graph present even with a modal open");
    let numeric = graph.numeric_value().expect("graph numeric value");
    assert!(
        (0.0..=numeric.maximum).contains(&numeric.current),
        "CPU value must stay clamped into the graph range"
    );
    for row in busy.nodes().filter(|node| node.role() == SemanticRole::Row) {
        assert!(row.name().is_some(), "every published row has a name");
        for cell_id in row.children() {
            assert!(
                busy.get(cell_id).is_some(),
                "every row child resolves to a node"
            );
        }
    }
}

/// (d) A pending process-termination confirmation is surfaced to
/// assistive technology: the live region announces the confirmation
/// (action + target pid) instead of the row count, and the tree stays
/// well-formed with the process table still published.
#[gpui::test]
async fn pending_termination_confirmation_is_published_to_the_live_region(cx: &mut TestAppContext) {
    let root = make_root(cx);
    root.update(cx, |view, _| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Apps;
        view.replace_processes_for_test(vec![process(3, "hog"), process(4, "idle")]);
        view.request_end_task_confirmation(identity(3));
    });

    let snapshot = snapshot_of(cx, &root, 11);
    assert!(
        snapshot.nodes().count() > 0,
        "a snapshot with a pending confirmation must stay well-formed"
    );

    let status = snapshot
        .get(&SemanticNodeId::borrowed("status"))
        .expect("status live region present");
    let announcement = status.name().unwrap_or_default();
    let announcement_lower = announcement.to_ascii_lowercase();
    assert!(
        announcement_lower.contains("confirming")
            && announcement_lower.contains("end task")
            && announcement.contains('3'),
        "the confirmation must be announced with its action and target pid, got {announcement:?}"
    );

    let rows: Vec<_> = snapshot
        .nodes()
        .filter(|node| node.role() == SemanticRole::Row)
        .collect();
    assert_eq!(rows.len(), 2, "the process table still publishes its rows");
    for row in rows {
        for cell_id in row.children() {
            assert!(
                snapshot.get(cell_id).is_some(),
                "every row child resolves to a node"
            );
        }
    }
}

/// The published row order is the shared process-sort authority on its CPU
/// axis (`taskmanager_application::process_sort`): highest CPU first, and the
/// pid tie-break makes equal readings deterministic — the AT reads the tree
/// top-down, so a local `partial_cmp` ordering would announce an
/// input-order-dependent list.
#[gpui::test]
async fn rows_are_published_highest_cpu_first_with_a_deterministic_tie_break(
    cx: &mut TestAppContext,
) {
    fn published_pids(cx: &mut TestAppContext, root: &Entity<RootView>) -> Vec<u32> {
        let snapshot = snapshot_of(cx, root, 5);
        let table = snapshot
            .get(&SemanticNodeId::borrowed("process-table"))
            .expect("process table present");
        table
            .children()
            .filter_map(|child| snapshot.get(child))
            .filter(|node| node.role() == SemanticRole::Row)
            .filter_map(|node| node.id().as_str().strip_prefix("row:process:pid:"))
            .filter_map(|rest| rest.split(':').next()?.parse::<u32>().ok())
            .collect()
    }

    let cpu_process = |pid: u32, name: &str, cpu: f32| {
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(pid)
            .name(name.into())
            .scalar_observations(ProcessScalarObservations {
                start_token: ScalarObservation::available(u64::from(pid) + 100, 1),
                ..ProcessScalarObservations::default()
            })
            .current_cpu_percentage(cpu)
            .build()
    };

    // Insertion order is deliberately the reverse of the published order.
    let root = make_root(cx);
    root.update(cx, |view, _| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Apps;
        view.replace_processes_for_test(vec![
            process(1003, "idle"),
            cpu_process(1002, "warm", 12.5),
            cpu_process(1001, "hog", 90.0),
        ]);
    });
    assert_eq!(
        published_pids(cx, &root),
        vec![1001, 1002, 1003],
        "the accessibility tree must publish the highest-CPU rows first"
    );

    // Equal readings: the pid tie-break, not the process-list order.
    let root = make_root(cx);
    root.update(cx, |view, _| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Apps;
        view.replace_processes_for_test(vec![
            cpu_process(4004, "beta", 4.0),
            cpu_process(2002, "alpha", 4.0),
        ]);
    });
    assert_eq!(
        published_pids(cx, &root),
        vec![2002, 4004],
        "equal CPU readings must fall back to the ascending pid tie-break"
    );
}

#[gpui::test]
async fn assistive_technology_process_actions_use_the_published_revision_and_selection_owner(
    cx: &mut TestAppContext,
) {
    let root = make_root(cx);
    root.update(cx, |view, _| {
        view.mark_telemetry_frame_ready();
        view.page = TopPage::Performance;
        view.replace_processes_for_test(vec![process(1001, "alpha"), process(2002, "bravo")]);
    });
    let snapshot = snapshot_of(cx, &root, 19);
    let request = AccessibilityActionRequest {
        snapshot_revision: snapshot.revision(),
        node: process_row_id(2002),
        action: SemanticAction::Select,
        value: None,
    };
    root.update(cx, |view, _| {
        apply_accessibility_action(view, &request, &snapshot).expect("matching AT action");
    });
    root.read_with(cx, |view, _| {
        assert_eq!(view.page, TopPage::Apps);
        assert!(view.is_process_selected(&process(2002, "bravo")));
    });

    let stale = AccessibilityActionRequest {
        snapshot_revision: snapshot.revision().saturating_sub(1),
        ..request
    };
    let rejection = root.update(cx, |view, _| {
        apply_accessibility_action(view, &stale, &snapshot).expect_err("stale AT action")
    });
    assert!(matches!(
        rejection,
        AccessibilityActionRejection::StaleSnapshot {
            current: 19,
            requested: 18
        }
    ));
}

#[gpui::test]
async fn assistive_technology_modal_dismissal_uses_the_shared_surface_owner(
    cx: &mut TestAppContext,
) {
    let root = make_root(cx);
    root.update(cx, |view, _| {
        view.mark_telemetry_frame_ready();
        view.replace_processes_for_test(vec![process(3, "hog")]);
        view.request_end_task_confirmation(identity(3));
    });
    let snapshot = snapshot_of(cx, &root, 23);
    let request = AccessibilityActionRequest {
        snapshot_revision: snapshot.revision(),
        node: SemanticNodeId::borrowed("modal:end-task-confirmation"),
        action: SemanticAction::Dismiss,
        value: None,
    };
    root.update(cx, |view, _| {
        apply_accessibility_action(view, &request, &snapshot).expect("matching modal dismiss");
        assert!(view.pending_confirmation().is_none());
    });
}
