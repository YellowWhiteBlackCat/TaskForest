use std::collections::HashSet;

use taskmanager_core::core::FailureKind;
use taskmanager_core::core::metrics::{ScalarAvailability, ScalarObservation};
use taskmanager_core::core::process::{
    ProcessApplicationIdentity, ProcessItem, ProcessMetadataObservation, ProcessScalarObservations,
};

use crate::{
    ProcessRowId, ProcessTreeRow, SortCol, SortDir, app_tree_expansion_key,
    project_process_tree_rows,
};

fn app_process(pid: u32, name: &str, parent_pid: Option<u32>, cpu: f32) -> ProcessItem {
    let identity =
        ProcessApplicationIdentity::new(format!("org.example.{name}"), name.to_owned(), None)
            .expect("test application identity");
    ProcessItem::new(pid, name)
        .with_application_identity_observation(ProcessMetadataObservation::available(identity, 10))
        .with_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(u64::from(pid) + 100, 10),
            cpu_percentage: ScalarObservation::available(cpu, 10),
            memory_bytes: ScalarObservation::available(u64::from(pid) * 10, 10),
            ..ProcessScalarObservations::default()
        })
        .tap_parent(parent_pid)
}

fn unknown_process(pid: u32) -> ProcessItem {
    ProcessItem::new(pid, "unknown").with_scalar_observations(ProcessScalarObservations {
        cpu_percentage: ScalarObservation::available(0.0, 10),
        memory_bytes: ScalarObservation::available(1, 10),
        ..ProcessScalarObservations::default()
    })
}

trait ParentFixture {
    fn tap_parent(self, parent_pid: Option<u32>) -> Self;
}

impl ParentFixture for ProcessItem {
    fn tap_parent(mut self, parent_pid: Option<u32>) -> Self {
        self.parent_pid = parent_pid;
        self
    }
}

fn rows(items: &[ProcessItem], expanded: &HashSet<String>) -> Vec<ProcessTreeRow> {
    let refs: Vec<_> = items.iter().collect();
    project_process_tree_rows(
        &refs,
        expanded,
        &HashSet::new(),
        (SortCol::Cpu, SortDir::Desc),
        10,
    )
}

#[test]
fn shared_projection_keeps_one_category_order_and_no_empty_headers() {
    let items = [app_process(10, "editor", None, 3.0), unknown_process(20)];
    let projected = rows(&items, &HashSet::new());
    let categories: Vec<_> = projected
        .iter()
        .filter_map(|row| match row {
            ProcessTreeRow::Category { category, .. } => Some(*category),
            _ => None,
        })
        .collect();
    assert_eq!(categories.len(), 2);
    assert_eq!(
        categories[0],
        taskmanager_core::core::process::ProcessCategory::Application
    );
    assert_eq!(
        categories[1],
        taskmanager_core::core::process::ProcessCategory::Uncategorized
    );
}

#[test]
fn shared_projection_retains_unknown_identity_rows_without_fabricating_targets() {
    let items = [unknown_process(20)];
    let expanded = HashSet::from(["category:uncategorized".to_owned()]);
    let projected = rows(&items, &expanded);
    let ProcessTreeRow::Process {
        visible_index,
        row_key,
        parent_key,
        depth,
        ..
    } = &projected[1]
    else {
        panic!("expanded category must expose its process row");
    };
    assert_eq!(*visible_index, 0);
    assert_eq!(*row_key, None);
    assert_eq!(
        *parent_key,
        Some(ProcessRowId::Category(
            taskmanager_core::core::process::ProcessCategory::Uncategorized
        ))
    );
    assert_eq!(*depth, 1);
}

#[test]
fn shared_projection_keeps_typed_unavailable_aggregate_visible() {
    let item = ProcessItem::new(30, "denied").with_scalar_observations(ProcessScalarObservations {
        start_token: ScalarObservation::available(130, 10),
        cpu_percentage: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        memory_bytes: ScalarObservation::unavailable(FailureKind::PermissionDenied),
        ..ProcessScalarObservations::default()
    });
    let projected = rows(&[item], &HashSet::new());
    let ProcessTreeRow::Category { aggregate, .. } = &projected[0] else {
        panic!("a non-empty category must produce a header");
    };
    assert_eq!(
        aggregate.cpu().availability(),
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(aggregate.cpu().current_value(), None);
    assert_eq!(aggregate.memory().current_value(), None);
}

#[test]
fn shared_projection_preserves_app_parent_and_aggregate_sort_order() {
    let first = app_process(10, "first", None, 1.0);
    let first_child = app_process(11, "first-child", Some(10), 50.0);
    let second = app_process(20, "second", None, 40.0);
    let second_child = app_process(21, "second-child", Some(20), 0.0);
    let items = [first, first_child, second, second_child];
    let expanded = HashSet::from([
        "category:application".to_owned(),
        app_tree_expansion_key(&items[0]),
        app_tree_expansion_key(&items[2]),
    ]);
    let projected = rows(&items, &expanded);
    let apps: Vec<_> = projected
        .iter()
        .filter_map(|row| match row {
            ProcessTreeRow::Application {
                row_key: Some(ProcessRowId::Application(key)),
                parent_key,
                aggregate,
                ..
            } => Some((
                key.pid(),
                *parent_key,
                aggregate.cpu().current_value().copied(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(apps[0].0, 10);
    assert_eq!(
        apps[0].1,
        ProcessRowId::Category(taskmanager_core::core::process::ProcessCategory::Application)
    );
    assert_eq!(apps[0].2, Some(51.0));
    assert_eq!(apps[1].0, 20);
    assert_eq!(apps[1].2, Some(40.0));
}
