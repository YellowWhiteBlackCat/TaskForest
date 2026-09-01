use super::*;
use taskmanager_application::process_category_projection::category_buckets;
use taskmanager_core::core::FailureKind;
use taskmanager_core::core::metrics::{ScalarAvailability, ScalarObservation};
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_core::core::process::{
    ProcessApplicationIdentity, ProcessMetadataObservation, process_category,
};

/// Fixture accepted-snapshot timestamp matching the observations below, whose
/// `last_success_ms` is 1.
const SNAPSHOT_MS: u64 = 1;

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
    kind: fn(ProcessLiveKey) -> taskmanager_shell::ProcessRowId,
    pid: u32,
) -> taskmanager_shell::ProcessRowId {
    ProcessLiveKey::from_parts(pid, u64::from(pid) * 10 + 1)
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
    let category_key = category_expansion_key(ProcessCategory::Application);
    assert!(!expansion.expanded_groups.contains(&category_key));
    let identity = key_of(taskmanager_shell::ProcessRowId::Process, 7)
        .live_key()
        .expect("fixture identity");
    let application_key = app_tree_expansion_key_for_identity(identity);
    assert!(!expansion.expanded_groups.contains(&application_key));
    assert!(!expansion.collapsed_processes.contains(&identity));

    expansion.toggle_category(ProcessCategory::Application);
    expansion.toggle_application(identity);
    expansion.toggle_process(identity);
    assert!(expansion.expanded_groups.contains(&category_key));
    assert!(expansion.expanded_groups.contains(&application_key));
    assert!(expansion.collapsed_processes.contains(&identity));

    expansion.toggle_category(ProcessCategory::Application);
    expansion.toggle_application(identity);
    expansion.toggle_process(identity);
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

    let collapsed = project_items(&items, &ProcessTreeExpansion::default(), SNAPSHOT_MS);
    assert_eq!(
        collapsed.iter().map(|row| row.key).collect::<Vec<_>>(),
        vec![
            Some(taskmanager_shell::ProcessRowId::Category(
                ProcessCategory::Application
            )),
            Some(taskmanager_shell::ProcessRowId::Category(
                ProcessCategory::Background
            )),
            Some(taskmanager_shell::ProcessRowId::Category(
                ProcessCategory::Uncategorized
            )),
        ]
    );
    assert_eq!(collapsed[0].member_count, 2);

    let mut expansion = ProcessTreeExpansion::default();
    expansion.toggle_category(ProcessCategory::Application);
    let category_open = project_items(&items, &expansion, SNAPSHOT_MS);
    assert_eq!(
        category_open[1].key,
        Some(key_of(taskmanager_shell::ProcessRowId::Application, 10))
    );
    assert_eq!(category_open[1].member_count, 2);

    expansion.toggle_application(
        key_of(taskmanager_shell::ProcessRowId::Process, 10)
            .live_key()
            .expect("root identity"),
    );
    let tree_open = project_items(&items, &expansion, SNAPSHOT_MS);
    assert_eq!(
        tree_open[2].key,
        Some(key_of(taskmanager_shell::ProcessRowId::Process, 10))
    );
    assert_eq!(
        tree_open[3].key,
        Some(key_of(taskmanager_shell::ProcessRowId::Process, 11))
    );
    assert_eq!(tree_open[2].item.map(|item| item.pid), Some(10));
    assert_eq!(tree_open[1].item, None);
    assert_eq!(tree_open[2].depth, 2);
    assert_eq!(tree_open[3].depth, 3);
}

#[test]
fn category_projection_consumes_typed_aggregate_without_zero_fallback() {
    let measured_zero = application(60, None, "Editor").with_scalar_observations(
        taskmanager_core::core::process::ProcessScalarObservations {
            cpu_percentage: ScalarObservation::available(0.0, 1),
            ..Default::default()
        },
    );
    let unavailable = application(61, None, "EditorWorker").with_scalar_observations(
        taskmanager_core::core::process::ProcessScalarObservations {
            cpu_percentage: ScalarObservation::unavailable(FailureKind::PermissionDenied),
            ..Default::default()
        },
    );
    let items = vec![measured_zero, unavailable];
    let buckets = category_buckets(&items, process_category);
    let application_bucket = buckets
        .iter()
        .find(|bucket| bucket.category() == ProcessCategory::Application)
        .expect("application category is present");

    let aggregate = application_bucket
        .aggregate_process_cpu(2)
        .expect("non-empty category has a typed aggregate");
    assert_eq!(aggregate.member_count(), 2);
    assert_eq!(aggregate.current_member_count(), 1);
    assert_eq!(aggregate.current_value(), Some(&0.0));
    assert_eq!(
        aggregate.availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
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
    expansion.toggle_application(
        key_of(taskmanager_shell::ProcessRowId::Process, 50)
            .live_key()
            .expect("root identity"),
    );
    expansion.toggle_process(
        key_of(taskmanager_shell::ProcessRowId::Process, 51)
            .live_key()
            .expect("child identity"),
    );
    let rows = project_items(&items, &expansion, SNAPSHOT_MS);
    assert_eq!(
        rows.iter().map(|row| row.key).collect::<Vec<_>>(),
        vec![
            Some(taskmanager_shell::ProcessRowId::Category(
                ProcessCategory::Application
            )),
            Some(key_of(taskmanager_shell::ProcessRowId::Application, 50)),
            Some(key_of(taskmanager_shell::ProcessRowId::Process, 50)),
            Some(key_of(taskmanager_shell::ProcessRowId::Process, 51)),
        ]
    );
    assert!(rows[3].has_children);
    assert!(!rows[3].expanded);
}

#[test]
fn tree_rows_carry_typed_aggregates_without_zero_fallback() {
    let mut measured = application(70, None, "Editor");
    let mut measured_observations = *measured.scalar_observations();
    measured_observations.cpu_percentage = ScalarObservation::available(0.0, SNAPSHOT_MS);
    measured_observations.memory_bytes = ScalarObservation::available(2_048, SNAPSHOT_MS);
    measured.apply_scalar_observations(measured_observations);
    let mut helper = application(71, Some(70), "Helper");
    let mut helper_observations = *helper.scalar_observations();
    helper_observations.cpu_percentage =
        ScalarObservation::unavailable(FailureKind::PermissionDenied);
    helper.apply_scalar_observations(helper_observations);
    measured.parent_pid = None;
    helper.parent_pid = Some(70);
    let background = tokened(80, "daemon", None).with_scalar_observations(
        taskmanager_core::core::process::ProcessScalarObservations {
            cpu_percentage: ScalarObservation::available(1.5, SNAPSHOT_MS),
            memory_bytes: ScalarObservation::available(4_096, SNAPSHOT_MS),
            ..Default::default()
        },
    );
    let items = vec![measured, helper, background];

    let mut expansion = ProcessTreeExpansion::default();
    expansion.toggle_category(ProcessCategory::Application);
    expansion.toggle_application(
        key_of(taskmanager_shell::ProcessRowId::Process, 70)
            .live_key()
            .expect("root identity"),
    );
    let rows = project_items(&items, &expansion, SNAPSHOT_MS);

    // Category header: partial CPU coverage keeps the measured member's zero
    // visible as a current zero, never collapsing into a fabricated total.
    // The identity-less daemon lands in another category and stays out here.
    let category = &rows[0];
    let category_cpu = category.cpu.as_ref().expect("category carries cpu");
    assert_eq!(category_cpu.member_count(), 2);
    assert_eq!(
        category_cpu.availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(category_cpu.current_value(), Some(&0.0));
    assert!(category.memory.is_some());

    // Application root: the typed group's current CPU excludes the
    // unavailable member; memory only counts the member that reported it.
    let app_root = &rows[1];
    let app_cpu = app_root.cpu.as_ref().expect("app root carries cpu");
    assert_eq!(app_root.member_count, 2);
    assert_eq!(app_cpu.member_count(), 2);
    assert_eq!(app_cpu.current_value(), Some(&0.0));
    assert_eq!(
        app_cpu.availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(
        app_root
            .memory
            .as_ref()
            .and_then(|memory| memory.current_value())
            .copied(),
        Some(2_048)
    );

    // Plain process rows aggregate nothing; they render their own scalars.
    let process_row = &rows[2];
    assert!(process_row.cpu.is_none());
    assert!(process_row.memory.is_none());
}
