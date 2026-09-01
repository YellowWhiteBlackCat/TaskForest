use super::*;
use std::collections::HashSet;

use taskmanager_core::core::FailureKind;
use taskmanager_core::core::metrics::{ScalarAvailability, ScalarObservation};
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::{SortCol, SortDir, project_process_tree_rows};

fn key(pid: u32) -> ProcessLiveKey {
    ProcessLiveKey::from_parts(pid, taskmanager_test_support::fixture_start_token(pid))
        .expect("fixture identity")
}

fn app_key(pid: u32) -> String {
    format!("app-tree:{}", key(pid).stable_key())
}

fn expected_key(
    kind: fn(ProcessLiveKey) -> taskmanager_shell::ProcessRowId,
    pid: u32,
) -> Option<taskmanager_shell::ProcessRowId> {
    ProcessLiveKey::from_parts(pid, taskmanager_test_support::fixture_start_token(pid)).map(kind)
}

fn proc(pid: u32, name: &str, cpu: f32, mem_mb: u64) -> ProcessItem {
    taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(pid)
        .name(name.into())
        .current_cpu_percentage(cpu)
        .current_memory_bytes(mem_mb * 1024 * 1024)
        .build()
}

fn app_proc(pid: u32, name: &str, cpu: f32, mem_mb: u64) -> ProcessItem {
    use taskmanager_core::core::process::{ProcessApplicationIdentity, ProcessMetadataObservation};
    let identity = ProcessApplicationIdentity::new("org.example.App", name, None)
        .expect("identity fixture needs real values");
    let mut item = proc(pid, name, cpu, mem_mb);
    item.apply_application_identity(ProcessMetadataObservation::available(identity, 10));
    item
}

fn background_proc(pid: u32, name: &str, cpu: f32, mem_mb: u64) -> ProcessItem {
    use taskmanager_core::core::process::{ProcessApplicationIdentity, ProcessMetadataObservation};
    let mut item = proc(pid, name, cpu, mem_mb);
    item.apply_application_identity(
        ProcessMetadataObservation::<ProcessApplicationIdentity>::absent(10),
    );
    item
}

fn rows<'a>(
    processes: &'a [&'a ProcessItem],
    expanded: &HashSet<String>,
    collapsed: &HashSet<ProcessLiveKey>,
) -> Vec<ProcessRow<'a>> {
    process_view_support::build_process_rows(
        processes,
        expanded,
        collapsed,
        (SortCol::Cpu, SortDir::Desc),
        10,
    )
}

#[test]
fn canonical_projection_buckets_in_domain_order_and_aggregates() {
    let processes = [
        app_proc(11, "editor", 24.8, 2_640),
        app_proc(12, "helper", 6.0, 800),
        background_proc(30, "daemon", 1.0, 10),
        proc(40, "unclassified", 0.4, 8),
    ];
    let refs: Vec<_> = processes.iter().collect();
    let projected = rows(&refs, &HashSet::new(), &HashSet::new());
    let headers: Vec<_> = projected
        .iter()
        .map(|row| match row {
            ProcessRow::Group {
                name,
                count,
                cpu,
                memory,
                ..
            } => (
                name.as_str(),
                *count,
                cpu.current_value().copied(),
                memory.current_value().copied(),
            ),
            ProcessRow::TreeNode { .. } => panic!("collapsed categories emit headers only"),
        })
        .collect();
    assert_eq!(
        headers.iter().map(|header| header.0).collect::<Vec<_>>(),
        [
            "category:application",
            "category:background",
            "category:uncategorized"
        ]
    );
    assert_eq!(headers[0].1, 2);
    assert!((headers[0].2.expect("CPU aggregate is current") - 30.8).abs() < 0.001);
    assert_eq!(headers[0].3, Some((2_640 + 800) * 1024 * 1024));
}

#[test]
fn group_rows_keep_typed_missing_metrics_instead_of_fabricating_zero() {
    let mut missing = ProcessItem::new(41, "missing");
    missing.apply_scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
        start_token: ScalarObservation::available(410, 10),
        cpu_percentage: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        memory_bytes: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        ..Default::default()
    });
    let refs = vec![&missing];
    let projected = rows(&refs, &HashSet::new(), &HashSet::new());

    let ProcessRow::Group { cpu, memory, .. } = &projected[0] else {
        panic!("a non-empty category must emit its typed header");
    };
    assert_eq!(
        cpu.availability(),
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(memory.availability(), cpu.availability());
    assert_eq!(cpu.current_value(), None);
    assert_eq!(memory.current_value(), None);
}

#[test]
fn application_aggregate_is_pidless_but_process_children_keep_identity() {
    let processes = [app_proc(11, "editor", 24.8, 2_640)];
    let refs: Vec<_> = processes.iter().collect();
    let expanded = HashSet::from(["category:application".to_string(), app_key(11)]);
    let projected = rows(&refs, &expanded, &HashSet::new());

    assert!(
        matches!(projected[1], ProcessRow::Group { depth: 1, .. })
            && crate::process_view::row_key_at(&projected, 1)
                == expected_key(taskmanager_shell::ProcessRowId::Application, 11)
    );
    assert!(matches!(
        projected[2],
        ProcessRow::TreeNode {
            process,
            depth: 2,
            ..
        } if process.pid == 11
    ));
    assert_eq!(
        row_key_at(&projected, 1),
        expected_key(taskmanager_shell::ProcessRowId::Application, 11)
    );
    assert_eq!(process_at(&projected, 1), None);
    assert_eq!(
        row_key_at(&projected, 2),
        expected_key(taskmanager_shell::ProcessRowId::Process, 11)
    );
}

#[test]
fn application_roots_sort_by_the_aggregate_header_metric() {
    let first = app_proc(11, "first", 1.0, 100);
    let mut first_child = app_proc(12, "first-child", 50.0, 100);
    first_child.parent_pid = Some(11);
    let second = app_proc(21, "second", 40.0, 100);
    let mut second_child = app_proc(22, "second-child", 0.0, 100);
    second_child.parent_pid = Some(21);

    let processes = [first, first_child, second, second_child];
    let refs: Vec<_> = processes.iter().collect();
    let expanded = HashSet::from(["category:application".to_owned(), app_key(11), app_key(21)]);
    let projected = process_view_support::build_process_rows(
        &refs,
        &expanded,
        &HashSet::new(),
        (SortCol::Cpu, SortDir::Desc),
        10,
    );
    let roots: Vec<_> = projected
        .iter()
        .filter_map(|row| match row {
            ProcessRow::Group {
                depth: 1,
                row_key: Some(taskmanager_shell::ProcessRowId::Application(identity)),
                ..
            } => Some(identity.pid()),
            _ => None,
        })
        .collect();
    assert_eq!(roots, [11, 21], "51% aggregate must precede 40% aggregate");
}

#[test]
fn collapsing_a_process_node_hides_its_recursive_subtree() {
    let parent = background_proc(1, "parent", 1.0, 10);
    let mut child = background_proc(2, "child", 2.0, 20);
    let mut grandchild = background_proc(3, "grandchild", 3.0, 30);
    child.parent_pid = Some(1);
    grandchild.parent_pid = Some(2);
    let processes = [parent, child, grandchild];
    let refs: Vec<_> = processes.iter().collect();
    let expanded = HashSet::from(["category:background".to_string()]);

    let open = rows(&refs, &expanded, &HashSet::new());
    let open_pids: Vec<_> = open
        .iter()
        .filter_map(|row| match row {
            ProcessRow::TreeNode { process, .. } => Some(process.pid),
            ProcessRow::Group { .. } => None,
        })
        .collect();
    assert_eq!(open_pids, [1, 2, 3]);

    let collapsed = rows(&refs, &expanded, &HashSet::from([key(1)]));
    let pids: Vec<_> = collapsed
        .iter()
        .filter_map(|row| match row {
            ProcessRow::TreeNode { process, .. } => Some(process.pid),
            ProcessRow::Group { .. } => None,
        })
        .collect();
    assert_eq!(pids, [1]);
}

#[test]
fn empty_fact_set_produces_no_structural_rows() {
    assert!(rows(&[], &HashSet::new(), &HashSet::new()).is_empty());
}

/// The exact owned summary of one row: every field the renderer and the
/// resolvers consume, with the f32 aggregate pinned at the bit level so even
/// a summation-order change cannot slip through. The cache-invariant tests
/// compare materialized rows against the fresh rebuild through this digest.
fn row_digest(row: &ProcessRow<'_>) -> String {
    match row {
        ProcessRow::Group {
            name,
            label,
            depth,
            count,
            cpu,
            memory,
            expanded,
            row_key,
        } => format!("G|{name}|{label}|{depth}|{count}|{cpu:?}|{memory:?}|{expanded}|{row_key:?}"),
        ProcessRow::TreeNode {
            process,
            depth,
            has_children,
            collapsed,
        } => format!(
            "T|{}|{}|{:?}|{depth}|{has_children}|{collapsed}",
            process.pid,
            process.name,
            process.current_start_token()
        ),
    }
}

fn digests(rows: &[ProcessRow<'_>]) -> Vec<String> {
    rows.iter().map(row_digest).collect()
}

/// The owned canonical-id slice must materialize to rows byte-identical to
/// the reference build, across the expansion/collapse shapes the page can
/// reach. This is the pure half of the cache-invariant contract; the TuiApp
/// hit/invalidation half lives with the runtime group-view tests.
#[test]
fn owned_id_slice_materializes_exactly_like_the_reference_build() {
    let mut child = background_proc(2, "child", 2.0, 20);
    child.parent_pid = Some(1);
    let mut grandchild = background_proc(3, "grandchild", 3.0, 30);
    grandchild.parent_pid = Some(2);
    let processes = [
        app_proc(11, "editor", 24.8, 2_640),
        app_proc(12, "helper", 6.0, 800),
        background_proc(1, "parent", 1.0, 10),
        child,
        grandchild,
        proc(40, "unclassified", 0.4, 8),
    ];
    let refs: Vec<_> = processes.iter().collect();
    let sort = (SortCol::Cpu, SortDir::Desc);

    let shapes: [(&HashSet<String>, &HashSet<ProcessLiveKey>); 3] = [
        (&HashSet::new(), &HashSet::new()),
        (
            &HashSet::from([
                "category:application".to_string(),
                "category:background".to_string(),
                "category:uncategorized".to_string(),
                app_key(11),
                app_key(1),
            ]),
            &HashSet::new(),
        ),
        (
            &HashSet::from([
                "category:application".to_string(),
                "category:background".to_string(),
                app_key(11),
            ]),
            &HashSet::from([key(1), key(12)]),
        ),
    ];

    for (expanded, collapsed) in shapes {
        let fresh = process_view_support::build_process_rows(&refs, expanded, collapsed, sort, 10);
        let ids = project_process_tree_rows(&refs, expanded, collapsed, sort, 10);
        let materialized = materialize_rows(&ids, &refs);
        assert_eq!(
            digests(&materialized),
            digests(&fresh),
            "materializing the owned ids must reproduce the reference rows \
             (expanded={expanded:?}, collapsed={collapsed:?})"
        );
        // And the pure builder is deterministic per input, so a cache that
        // stores one emission can never diverge from a later rebuild under
        // the same key.
        let rebuilt = project_process_tree_rows(&refs, expanded, collapsed, sort, 10);
        assert_eq!(ids, rebuilt, "the id build must be a pure function");
    }
}
