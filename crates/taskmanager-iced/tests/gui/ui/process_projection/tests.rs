use super::*;

/// Expected row id for a fixture process (single source: the fixture
/// builder's default start token).
fn expected_row_key(
    kind: fn(taskmanager_shell::ProcessRowIdentity) -> taskmanager_shell::ProcessRowId,
    pid: u32,
) -> Option<taskmanager_shell::ProcessRowId> {
    taskmanager_shell::ProcessRowIdentity::from_parts(
        pid,
        taskmanager_test_support::fixture_start_token(pid),
    )
    .map(kind)
}

fn proc(pid: u32, name: &str, parent_pid: Option<u32>) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.into())
        .parent_pid(parent_pid)
        .current_cpu_percentage(pid as f32)
        .build()
}

fn app_proc(pid: u32, name: &str) -> ProcessItem {
    use taskmanager_core::core::process::{ProcessApplicationIdentity, ProcessMetadataObservation};

    let identity =
        ProcessApplicationIdentity::new("org.example.App", name, None).expect("identity fixture");
    let mut process = proc(pid, name, None);
    process.apply_application_identity(ProcessMetadataObservation::available(identity, 10));
    process
}

fn project(
    items: &[ProcessItem],
    expanded_groups: &HashSet<String>,
    collapsed: &HashSet<u32>,
) -> ProcessProjection {
    let refs: Vec<_> = items.iter().collect();
    ProcessProjection::project_with_local_time(
        &refs,
        (SortCol::Cpu, SortDir::Desc),
        expanded_groups,
        collapsed,
        &taskmanager_core::core::time::LocalTimeRulesObservation::unsupported(0),
    )
}

#[test]
fn category_projection_has_one_fixed_bucket_order() {
    use taskmanager_core::core::process::{ProcessApplicationIdentity, ProcessMetadataObservation};

    taskmanager_test_support::pin_english();
    let mut background = proc(20, "daemon", None);
    background.apply_application_identity(
        ProcessMetadataObservation::<ProcessApplicationIdentity>::absent(10),
    );
    let items = [
        app_proc(10, "editor"),
        background,
        proc(30, "unknown", None),
    ];
    let projection = project(&items, &HashSet::new(), &HashSet::new());
    let names: Vec<_> = projection
        .rows()
        .iter()
        .map(|row| match row {
            ProjectedRow::GroupHeader { name, .. } => name.as_str(),
            ProjectedRow::Tree { .. } => panic!("collapsed categories emit headers only"),
        })
        .collect();
    assert_eq!(
        names,
        ["Applications", "Background processes", "Uncategorized"]
    );
}

#[test]
fn application_total_and_recursive_process_rows_keep_distinct_identity() {
    let items = [app_proc(10, "editor")];
    let expanded = HashSet::from([
        "category:application".to_string(),
        "app-tree:10".to_string(),
    ]);
    let projection = project(&items, &expanded, &HashSet::new());
    assert_eq!(
        projection.rows()[1].row_key(),
        expected_row_key(taskmanager_shell::ProcessRowId::Application, 10)
    );
    assert_eq!(
        projection.rows()[2].row_key(),
        expected_row_key(taskmanager_shell::ProcessRowId::Process, 10)
    );
}

#[test]
fn collapsed_tree_node_hides_only_its_descendants() {
    let items = [
        proc(1, "root", None),
        proc(2, "child", Some(1)),
        proc(3, "grandchild", Some(2)),
    ];
    let expanded = HashSet::from(["category:uncategorized".to_string()]);
    let projection = project(&items, &expanded, &HashSet::from([1]));
    let pids: Vec<_> = projection
        .rows()
        .iter()
        .filter_map(|row| match row {
            ProjectedRow::Tree { pid, .. } => Some(*pid),
            ProjectedRow::GroupHeader { .. } => None,
        })
        .collect();
    assert_eq!(pids, [1]);
}

#[test]
fn fingerprint_changes_for_every_runtime_projection_input() {
    let groups = HashSet::from(["category:uncategorized".to_string()]);
    let tree = HashSet::new();
    let base = ProcessProjectionFingerprint::build_with_status(
        1,
        ProcessStatusFilter::All,
        (SortCol::Cpu, SortDir::Desc),
        "",
        &groups,
        &tree,
    );
    assert_eq!(
        base,
        ProcessProjectionFingerprint::build_with_status(
            1,
            ProcessStatusFilter::All,
            (SortCol::Cpu, SortDir::Desc),
            "",
            &groups,
            &tree,
        )
    );
    assert_ne!(
        base,
        ProcessProjectionFingerprint::build_with_status(
            2,
            ProcessStatusFilter::All,
            (SortCol::Cpu, SortDir::Desc),
            "",
            &groups,
            &tree,
        )
    );
    assert_ne!(
        base,
        ProcessProjectionFingerprint::build_with_status(
            1,
            ProcessStatusFilter::Running,
            (SortCol::Cpu, SortDir::Desc),
            "",
            &groups,
            &tree,
        )
    );
    assert_ne!(
        base,
        ProcessProjectionFingerprint::build_with_status(
            1,
            ProcessStatusFilter::All,
            (SortCol::Memory, SortDir::Asc),
            "needle",
            &groups,
            &HashSet::from([1]),
        )
    );
}

#[test]
fn row_cells_keep_unavailable_values_honest() {
    let mut process = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(7)
        .name("missing".into())
        .build();
    let mut observations = *process.scalar_observations();
    observations.memory_bytes = taskmanager_core::core::metrics::ScalarObservation::unavailable(
        taskmanager_core::core::failure::FailureKind::PermissionDenied,
    );
    process.apply_scalar_observations(observations);
    let cells = build_row_cells(&process);
    assert_eq!(cells.pid, "7");
    assert_eq!(cells.memory, taskmanager_shell::presentation::MISSING_VALUE);
    assert_eq!(cells.pss, taskmanager_shell::presentation::MISSING_VALUE);
}
