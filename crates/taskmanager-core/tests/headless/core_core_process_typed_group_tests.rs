use super::*;
use crate::core::process::group_aggregate::{
    aggregate_apps_typed, aggregate_by_type_typed, aggregate_by_user_typed,
    aggregate_process_group_typed,
};
use crate::core::{FailureKind, ScalarAvailability, ScalarObservation};

fn process(
    pid: u32,
    name: &str,
    cpu: ScalarObservation<f32>,
    memory: ScalarObservation<u64>,
) -> ProcessItem {
    ProcessItem::new(pid, name).with_scalar_observations(ProcessScalarObservations {
        cpu_percentage: cpu,
        memory_bytes: memory,
        start_token: ScalarObservation::available(u64::from(pid) + 100, 1),
        ..ProcessScalarObservations::default()
    })
}

#[test]
fn typed_application_groups_keep_partial_coverage_and_member_counts() {
    let items = [
        process(
            10,
            "chrome",
            ScalarObservation::available(4.0, 1),
            ScalarObservation::available(100, 1),
        ),
        process(
            11,
            "chrome",
            ScalarObservation::unavailable(FailureKind::PermissionDenied),
            ScalarObservation::unavailable(FailureKind::PermissionDenied),
        ),
    ];
    let refs: Vec<&ProcessItem> = items.iter().collect();
    let groups = aggregate_apps_typed(&refs, 42);

    let group = groups
        .iter()
        .find(|group| group.name() == "Google Chrome")
        .expect("chrome group");
    assert_eq!(group.process_count(), 2);
    assert_eq!(
        group.member_identities(),
        &[
            ProcessLiveKey::from_parts(10, 110).expect("fixture identity"),
            ProcessLiveKey::from_parts(11, 111).expect("fixture identity"),
        ]
    );
    assert_eq!(
        group.cpu().availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(group.cpu().current_value(), Some(&4.0));
    assert_eq!(group.cpu().current_member_count(), 1);
    assert_eq!(group.cpu().known_member_count(), 1);
    assert_eq!(
        group.memory().availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(group.memory().current_value(), Some(&100));
}

#[test]
fn arbitrary_members_builder_keeps_tree_root_and_aggregate_authority_together() {
    let root = process(
        50,
        "tree-root",
        ScalarObservation::available(2.0, 1),
        ScalarObservation::available(20, 1),
    );
    let child = process(
        51,
        "tree-child",
        ScalarObservation::available(1.0, 1),
        ScalarObservation::available(10, 1),
    );
    let members = [&root, &child];

    let root_identity = ProcessLiveKey::from_parts(50, 150).expect("fixture identity");
    let group = aggregate_process_group_typed("tree", Some(root_identity), None, &members, 42)
        .expect("root belongs to the non-empty member set");
    assert_eq!(group.name(), "tree");
    assert_eq!(group.main_identity(), Some(root_identity));
    assert_eq!(
        group.member_identities(),
        &[
            ProcessLiveKey::from_parts(50, 150).expect("fixture identity"),
            ProcessLiveKey::from_parts(51, 151).expect("fixture identity"),
        ]
    );
    assert_eq!(group.cpu().current_value(), Some(&3.0));
    assert_eq!(group.memory().current_value(), Some(&30));

    assert!(
        aggregate_process_group_typed(
            "missing-root",
            Some(ProcessLiveKey::from_parts(99, 199).expect("fixture identity")),
            None,
            &members,
            42
        )
        .is_none()
    );
    assert!(aggregate_process_group_typed("empty", Some(root_identity), None, &[], 42).is_none());
}

#[test]
fn typed_groups_keep_unknown_distinct_from_a_measured_zero() {
    let items = [process(
        20,
        "zed",
        ScalarObservation::default(),
        ScalarObservation::default(),
    )];
    let refs: Vec<&ProcessItem> = items.iter().collect();

    let typed = aggregate_apps_typed(&refs, 42);
    let typed_group = typed.first().expect("non-empty group");
    assert_eq!(
        typed_group.cpu().availability(),
        ScalarAvailability::Unknown
    );
    assert_eq!(typed_group.cpu().current_value(), None);
    assert_eq!(
        typed_group.memory().availability(),
        ScalarAvailability::Unknown
    );
    assert_eq!(typed_group.memory().current_value(), None);
}

#[test]
fn typed_user_groups_preserve_a_partial_metric_without_fabricating_success() {
    let items = vec![
        process(
            30,
            "worker-a",
            ScalarObservation::available(3.0, 1),
            ScalarObservation::available(50, 1),
        ),
        process(
            31,
            "worker-b",
            ScalarObservation::unavailable(FailureKind::TimedOut),
            ScalarObservation::unavailable(FailureKind::TimedOut),
        ),
    ];
    let groups = aggregate_by_user_typed(&items, 42);
    let group = groups.first().expect("missing-owner bucket");

    assert_eq!(group.user(), None);
    assert_eq!(group.process_count(), 2);
    assert_eq!(
        group.cpu().availability(),
        ScalarAvailability::Partial(FailureKind::TimedOut)
    );
    assert_eq!(group.cpu().current_value(), Some(&3.0));
    assert_eq!(
        group.memory().availability(),
        ScalarAvailability::Partial(FailureKind::TimedOut)
    );
    assert_eq!(group.memory().current_value(), Some(&50));
}

#[test]
fn typed_process_class_groups_sort_current_values_before_unavailable_groups() {
    let items = [
        process(
            40,
            "[kworker]",
            ScalarObservation::unavailable(FailureKind::Unsupported),
            ScalarObservation::unavailable(FailureKind::Unsupported),
        ),
        process(
            41,
            "worker",
            ScalarObservation::available(1.0, 1),
            ScalarObservation::available(10, 1),
        ),
    ];
    let refs: Vec<&ProcessItem> = items.iter().collect();
    let groups = aggregate_by_type_typed(&refs, 42);

    assert_eq!(groups[0].name(), "Userspace");
    assert_eq!(groups[0].cpu().current_value(), Some(&1.0));
    assert_eq!(groups[1].name(), "Kernel");
    assert_eq!(
        groups[1].cpu().availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(groups[1].cpu().current_value(), None);
}
