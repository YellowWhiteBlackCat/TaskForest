use super::*;

fn expected_key(
    kind: fn(taskmanager_shell::ProcessRowIdentity) -> taskmanager_shell::ProcessRowId,
    pid: u32,
) -> Option<taskmanager_shell::ProcessRowId> {
    taskmanager_shell::ProcessRowIdentity::from_parts(
        pid,
        taskmanager_test_support::fixture_start_token(pid),
    )
    .map(kind)
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
    collapsed: &HashSet<u32>,
) -> Vec<ProcessRow<'a>> {
    build_process_rows(
        processes,
        expanded,
        collapsed,
        (SortCol::Cpu, SortDir::Desc),
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
            } => (name.as_str(), *count, *cpu, *memory),
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
    assert!((headers[0].2 - 30.8).abs() < 0.001);
    assert_eq!(headers[0].3, (2_640 + 800) * 1024 * 1024);
}

#[test]
fn application_aggregate_is_pidless_but_process_children_keep_identity() {
    let processes = [app_proc(11, "editor", 24.8, 2_640)];
    let refs: Vec<_> = processes.iter().collect();
    let expanded = HashSet::from([
        "category:application".to_string(),
        "app-tree:11".to_string(),
    ]);
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

    let collapsed = rows(&refs, &expanded, &HashSet::from([1]));
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
        } => format!(
            "G|{name}|{label}|{depth}|{count}|{:?}|{memory}|{expanded}|{row_key:?}",
            cpu.to_bits()
        ),
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

    let shapes: [(&HashSet<String>, &HashSet<u32>); 3] = [
        (&HashSet::new(), &HashSet::new()),
        (
            &HashSet::from([
                "category:application".to_string(),
                "category:background".to_string(),
                "category:uncategorized".to_string(),
                "app-tree:11".to_string(),
                "app-tree:1".to_string(),
            ]),
            &HashSet::new(),
        ),
        (
            &HashSet::from([
                "category:application".to_string(),
                "category:background".to_string(),
                "app-tree:11".to_string(),
            ]),
            &HashSet::from([1u32, 12]),
        ),
    ];

    for (expanded, collapsed) in shapes {
        let fresh = build_process_rows(&refs, expanded, collapsed, sort);
        let ids = build_canonical_row_ids(&refs, expanded, collapsed, sort);
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
        let rebuilt = build_canonical_row_ids(&refs, expanded, collapsed, sort);
        assert_eq!(ids, rebuilt, "the id build must be a pure function");
    }
}
