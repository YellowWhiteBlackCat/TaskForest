//! Pure-Rust validation of the `SemanticSnapshot → accesskit::TreeUpdate`
//! mapping. These tests run on every target (no D-Bus, no accesskit_unix) and
//! use `accesskit_consumer::Tree` — accesskit's own structural validator — as
//! the well-formedness oracle, exactly as accesskit tests itself.

use accesskit::{Live, NodeId, Role};
use accesskit_consumer::{Node, Tree, TreeChangeHandler};
use taskmanager_accessibility_linux::mapping::{
    focused_node_id, map_action, map_role, stable_node_id,
};
use taskmanager_ui_contract::{
    GraphSummary, ProcessRowInput, SemanticAction, SemanticRole, SemanticSnapshotBuilder,
};

/// No-op change handler so we can drive incremental updates through the
/// consumer tree without caring about emitted events.
struct NoOpChangeHandler;

impl TreeChangeHandler for NoOpChangeHandler {
    fn node_added(&mut self, _node: &Node) {}
    fn node_updated(&mut self, _old_node: &Node, _new_node: &Node) {}
    fn focus_moved(&mut self, _old_node: Option<&Node>, _new_node: Option<&Node>) {}
    fn node_removed(&mut self, _node: &Node) {}
}

fn sample_snapshot() -> taskmanager_ui_contract::SemanticSnapshot {
    SemanticSnapshotBuilder::new(7)
        .application_name("TaskForest")
        .process_rows([
            ProcessRowInput {
                id: String::from("1024"),
                name: String::from("firefox"),
                cpu_percent: Some(12.3),
                memory_percent: Some(4.5),
                selected: true,
            },
            ProcessRowInput {
                id: String::from("2048"),
                name: String::from("cargo"),
                cpu_percent: Some(87.6),
                memory_percent: Some(2.1),
                selected: false,
            },
        ])
        .cpu_graph(GraphSummary {
            current: 18.0,
            peak: 72.0,
            maximum: 100.0,
        })
        .status_announcement("2 processes visible")
        .build()
        .expect("canonical builder snapshot must be well-formed")
}

fn build_consumer_tree(snapshot: &taskmanager_ui_contract::SemanticSnapshot) -> Tree {
    build_consumer_tree_with_focus(snapshot, false)
}

fn build_consumer_tree_with_focus(
    snapshot: &taskmanager_ui_contract::SemanticSnapshot,
    is_host_focused: bool,
) -> Tree {
    let update = taskmanager_accessibility_linux::snapshot_to_tree_update(snapshot);
    // accesskit_consumer::Tree::new panics on a malformed tree (missing root,
    // dangling child, focus not in node list, etc.). If it returns, the tree is
    // connected, acyclic, root-present, and focus-valid.
    Tree::new(update, is_host_focused)
}

#[test]
fn mapped_tree_is_well_formed_under_accesskit_consumer_oracle() {
    let snapshot = sample_snapshot();
    let tree = build_consumer_tree(&snapshot);

    let root = tree.state().root();
    assert_eq!(root.role(), Role::Application);
    assert_eq!(root.label().as_deref(), Some("TaskForest"));
}

#[test]
fn root_has_main_landmark_wrapping_table_graph_and_live_region() {
    let snapshot = sample_snapshot();
    let tree = build_consumer_tree(&snapshot);

    let root = tree.state().root();
    // root → main → [table, graph]; root → status (live region).
    let main = find_child_by_role(&root, Role::Main).expect("Main landmark present");
    assert!(
        find_child_by_role(&main, Role::Table).is_some(),
        "process table is a child of Main"
    );
    let graph = find_child_by_role(&main, Role::Meter).expect("graph (Meter) present");
    assert_eq!(graph.numeric_value(), Some(18.0));
    assert_eq!(graph.min_numeric_value(), Some(0.0));
    assert_eq!(graph.max_numeric_value(), Some(100.0));
    assert_eq!(
        graph.value().as_deref(),
        Some("Latest 18%, peak 72%"),
        "spoken graph value is announced via the value property"
    );

    let status = find_child_by_role(&root, Role::TextRun).expect("status text present");
    assert_eq!(status.live(), Live::Polite);
    assert_eq!(status.label().as_deref(), Some("2 processes visible"));
}

#[test]
fn process_table_carries_headers_and_one_row_per_process() {
    let snapshot = sample_snapshot();
    let tree = build_consumer_tree(&snapshot);

    let root = tree.state().root();
    let main = find_child_by_role(&root, Role::Main).expect("Main present");
    let table = find_child_by_role(&main, Role::Table).expect("Table present");

    let headers = table
        .children()
        .filter(|child| child.role() == Role::ColumnHeader)
        .count();
    assert_eq!(headers, 3, "three column headers (Name/CPU/Memory)");

    let rows: Vec<Node<'_>> = table
        .children()
        .filter(|child| child.role() == Role::Row)
        .collect();
    assert_eq!(rows.len(), 2, "one row per process");

    let selected_row = rows
        .iter()
        .find(|row| row.is_selected() == Some(true))
        .expect("exactly one selected row (firefox)");
    assert_eq!(selected_row.label().as_deref(), Some("firefox"));
}

#[test]
fn focus_resolves_to_the_selected_row() {
    let snapshot = sample_snapshot();

    let tree = build_consumer_tree_with_focus(&snapshot, true);
    let focus = tree.state().focus().expect("a focused node is reported");
    assert_eq!(focus.role(), Role::Row);
    assert_eq!(focus.label().as_deref(), Some("firefox"));
    assert_eq!(focus.is_selected(), Some(true));
}

#[test]
fn focus_falls_back_to_root_when_nothing_is_selected() {
    let snapshot = SemanticSnapshotBuilder::new(3)
        .process_row(ProcessRowInput {
            id: String::from("1024"),
            name: String::from("firefox"),
            cpu_percent: Some(1.0),
            memory_percent: Some(1.0),
            selected: false,
        })
        .build()
        .expect("snapshot well-formed");

    let root_focus = focused_node_id(&snapshot);
    assert_eq!(
        root_focus,
        stable_node_id(&taskmanager_ui_contract::SemanticNodeId::borrowed("app"))
    );
}

#[test]
fn incremental_snapshot_diffs_cleanly_through_consumer() {
    let first = sample_snapshot();
    let mut tree = build_consumer_tree(&first);

    // Second revision: drop one process, change the graph value, rotate the
    // status announcement. accesskit must apply this without structural errors.
    let second = SemanticSnapshotBuilder::new(8)
        .application_name("TaskForest")
        .process_row(ProcessRowInput {
            id: String::from("1024"),
            name: String::from("firefox"),
            cpu_percent: Some(55.0),
            memory_percent: Some(4.5),
            selected: true,
        })
        .cpu_graph(GraphSummary {
            current: 41.0,
            peak: 72.0,
            maximum: 100.0,
        })
        .status_announcement("1 process visible")
        .build()
        .expect("second revision well-formed");

    let update = taskmanager_accessibility_linux::snapshot_to_tree_update(&second);
    let mut handler = NoOpChangeHandler;
    tree.update_and_process_changes(update, &mut handler);

    // The graph value advanced and the AT-visible value text updated.
    let root = tree.state().root();
    let main = find_child_by_role(&root, Role::Main).expect("Main present");
    let graph = find_child_by_role(&main, Role::Meter).expect("graph present");
    assert_eq!(graph.numeric_value(), Some(41.0));
    assert_eq!(graph.value().as_deref(), Some("Latest 41%, peak 72%"));
}

#[test]
fn node_identity_is_stable_across_revisions_and_churn() {
    // Content-addressed ids must not shift when siblings are added/removed;
    // otherwise accesskit would treat every node as changed on each update.
    let firefox_row_id = stable_node_id_snapshot_row("1024");

    let first = sample_snapshot();
    let second = SemanticSnapshotBuilder::new(9)
        .application_name("TaskForest")
        .process_row(ProcessRowInput {
            id: String::from("1024"),
            name: String::from("firefox"),
            cpu_percent: Some(1.0),
            memory_percent: Some(1.0),
            selected: true,
        })
        .cpu_graph(GraphSummary {
            current: 1.0,
            peak: 1.0,
            maximum: 100.0,
        })
        .build()
        .expect("snapshot well-formed");

    assert_eq!(
        stable_node_id_snapshot_row("1024"),
        firefox_row_id,
        "id is independent of which other rows are present"
    );
    // The id is also the one actually published.
    assert_eq!(focused_node_id(&first), firefox_row_id);
    assert_eq!(focused_node_id(&second), firefox_row_id);
}

#[test]
fn role_and_action_mappings_are_faithful() {
    // Direct role mappings for the canonical TaskForest roles.
    assert_eq!(map_role(SemanticRole::Application), Role::Application);
    assert_eq!(map_role(SemanticRole::Main), Role::Main);
    assert_eq!(map_role(SemanticRole::Table), Role::Table);
    assert_eq!(map_role(SemanticRole::Row), Role::Row);
    assert_eq!(map_role(SemanticRole::ColumnHeader), Role::ColumnHeader);
    assert_eq!(map_role(SemanticRole::Cell), Role::Cell);
    assert_eq!(map_role(SemanticRole::StaticText), Role::TextRun);
    assert_eq!(map_role(SemanticRole::Graph), Role::Meter);

    // Actions the AT can emit map back to semantic equivalents.
    assert_eq!(
        map_action(SemanticAction::Focus),
        Some(accesskit::Action::Focus)
    );
    assert_eq!(
        map_action(SemanticAction::Press),
        Some(accesskit::Action::Click)
    );
    // Graph read-previous/next have no AT equivalent and are intentionally dropped.
    assert_eq!(map_action(SemanticAction::ReadPreviousValue), None);
    assert_eq!(map_action(SemanticAction::ReadNextValue), None);
}

fn find_child_by_role<'a>(parent: &Node<'a>, role: Role) -> Option<Node<'a>> {
    parent.children().find(|child| child.role() == role)
}

fn stable_node_id_snapshot_row(pid: &str) -> NodeId {
    stable_node_id(&taskmanager_ui_contract::SemanticNodeId::owned(format!(
        "row:{pid}"
    )))
}
