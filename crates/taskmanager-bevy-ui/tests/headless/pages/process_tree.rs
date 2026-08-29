use super::*;
use taskmanager_core::core::process::{ProcessApplicationIdentity, ProcessMetadataObservation};

fn tokened(
    pid: u32,
    name: &str,
    parent: Option<u32>,
) -> taskmanager_core::core::process::ProcessItem {
    use taskmanager_core::core::metrics::ScalarObservation;
    use taskmanager_core::core::process::ProcessScalarObservations;
    let mut item = taskmanager_core::core::process::ProcessItem::new(pid, name);
    item.parent_pid = parent;
    item.with_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid) * 10 + 1, 1),
        ..ProcessScalarObservations::default()
    })
}

fn key_of(
    kind: fn(taskmanager_shell::ProcessRowIdentity) -> taskmanager_shell::ProcessRowId,
    pid: u32,
) -> taskmanager_shell::ProcessRowId {
    taskmanager_shell::ProcessRowIdentity::from_parts(pid, u64::from(pid) * 10 + 1)
        .map(kind)
        .expect("non-zero parts")
}

fn application(pid: u32, parent_pid: Option<u32>, name: &str) -> ProcessItem {
    use taskmanager_core::core::metrics::ScalarObservation;
    use taskmanager_core::core::process::ProcessScalarObservations;

    let identity =
        ProcessApplicationIdentity::new(format!("org.example.{name}"), name.to_owned(), None)
            .expect("fixture identity is non-empty");
    let mut item = ProcessItem::new(pid, name)
        .with_application_identity_observation(ProcessMetadataObservation::available(identity, 1));
    item.parent_pid = parent_pid;
    item.apply_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(u64::from(pid) * 10 + 1, 1),
        ..ProcessScalarObservations::default()
    });
    item
}

#[test]
fn expansion_keys_are_stable_and_typed() {
    let mut expansion = ProcessTreeExpansion::default();
    assert!(!expansion.category_expanded(ProcessCategory::Application));
    assert!(!expansion.application_expanded(7));
    assert!(!expansion.process_collapsed(7));

    expansion.toggle_category(ProcessCategory::Application);
    expansion.toggle_application(7);
    expansion.toggle_process(7);
    assert!(expansion.category_expanded(ProcessCategory::Application));
    assert!(expansion.application_expanded(7));
    assert!(expansion.process_collapsed(7));

    expansion.toggle_category(ProcessCategory::Application);
    expansion.toggle_application(7);
    expansion.toggle_process(7);
    assert_eq!(expansion, ProcessTreeExpansion::default());
}

#[test]
fn projection_keeps_category_aggregate_and_process_identities_distinct() {
    let mut root = application(10, None, "Editor");
    let mut child = application(11, Some(10), "EditorWorker");
    let background = tokened(20, "daemon", None)
        .with_application_identity_observation(ProcessMetadataObservation::absent(1));
    let unknown = tokened(30, "unresolved", None);
    root.parent_pid = None;
    child.parent_pid = Some(10);
    let items = vec![root, child, background, unknown];

    let collapsed = project_items(&items, &ProcessTreeExpansion::default());
    assert_eq!(
        collapsed.iter().map(|row| row.key).collect::<Vec<_>>(),
        vec![
            taskmanager_shell::ProcessRowId::Category(ProcessCategory::Application),
            taskmanager_shell::ProcessRowId::Category(ProcessCategory::Background),
            taskmanager_shell::ProcessRowId::Category(ProcessCategory::Uncategorized),
        ]
    );
    assert_eq!(collapsed[0].member_count, 2);

    let mut expansion = ProcessTreeExpansion::default();
    expansion.toggle_category(ProcessCategory::Application);
    let category_open = project_items(&items, &expansion);
    assert_eq!(
        category_open[1].key,
        key_of(taskmanager_shell::ProcessRowId::Application, 10)
    );
    assert_eq!(category_open[1].member_count, 2);

    expansion.toggle_application(10);
    let tree_open = project_items(&items, &expansion);
    assert_eq!(
        tree_open[2].key,
        key_of(taskmanager_shell::ProcessRowId::Process, 10)
    );
    assert_eq!(
        tree_open[3].key,
        key_of(taskmanager_shell::ProcessRowId::Process, 11)
    );
    assert_eq!(tree_open[2].item.map(|item| item.pid), Some(10));
    assert_eq!(tree_open[1].item, None);
    assert_eq!(tree_open[2].depth, 2);
    assert_eq!(tree_open[3].depth, 3);
}

#[test]
fn process_collapse_hides_only_descendants() {
    let mut root = application(50, None, "Editor");
    let mut child = application(51, Some(50), "Worker");
    let mut grandchild = application(52, Some(51), "Helper");
    root.parent_pid = None;
    child.parent_pid = Some(50);
    grandchild.parent_pid = Some(51);
    let items = vec![root, child, grandchild];

    let mut expansion = ProcessTreeExpansion::default();
    expansion.toggle_category(ProcessCategory::Application);
    expansion.toggle_application(50);
    expansion.toggle_process(51);
    let rows = project_items(&items, &expansion);
    assert_eq!(
        rows.iter().map(|row| row.key).collect::<Vec<_>>(),
        vec![
            taskmanager_shell::ProcessRowId::Category(ProcessCategory::Application),
            key_of(taskmanager_shell::ProcessRowId::Application, 50),
            key_of(taskmanager_shell::ProcessRowId::Process, 50),
            key_of(taskmanager_shell::ProcessRowId::Process, 51),
        ]
    );
    assert!(rows[3].has_children);
    assert!(!rows[3].expanded);
}
