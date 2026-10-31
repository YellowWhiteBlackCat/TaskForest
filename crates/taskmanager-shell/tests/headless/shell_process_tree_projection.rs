use std::collections::HashSet;

use taskforest_core::core::FailureKind;
use taskforest_core::core::metrics::{ScalarAvailability, ScalarObservation};
use taskforest_core::core::process::{
    ProcessApplicationIdentity, ProcessItem, ProcessMetadataObservation, ProcessScalarObservations,
};

use crate::{
    ProcessProjectionGeneration, ProcessRowId, ProcessTreeRow, SortCol, SortDir,
    app_tree_expansion_key, append_process_semantic_rows, project_process_tree_rows,
};
use taskforest_core::core::process::{ProcessCategory, ProcessLiveKey};
use taskmanager_ui_contract::{
    SemanticAction, SemanticNodeId, SemanticRole, SemanticSnapshotBuilder,
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
    assert_eq!(categories[0], ProcessCategory::Application);
    assert_eq!(categories[1], ProcessCategory::Uncategorized);
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
        Some(ProcessRowId::Category(ProcessCategory::Uncategorized))
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
        ProcessRowId::Category(ProcessCategory::Application)
    );
    assert_eq!(apps[0].2, Some(51.0));
    assert_eq!(apps[1].0, 20);
    assert_eq!(apps[1].2, Some(40.0));
}

#[test]
fn semantic_adapter_appends_typed_rows_in_canonical_order() {
    let root = app_process(10, "editor", None, 3.0);
    let child = app_process(11, "editor-child", Some(10), 50.0);
    let unknown = unknown_process(20);
    let items = [root, child, unknown];
    let expanded = HashSet::from([
        "category:application".to_owned(),
        app_tree_expansion_key(&items[0]),
        "category:uncategorized".to_owned(),
    ]);
    let visible: Vec<_> = items.iter().collect();
    let rows = project_process_tree_rows(
        &visible,
        &expanded,
        &HashSet::new(),
        (SortCol::Cpu, SortDir::Desc),
        10,
    );
    let root_key = ProcessLiveKey::from_process(&items[0]).expect("root identity");
    let generation = ProcessProjectionGeneration::new(44);
    let snapshot = append_process_semantic_rows(
        SemanticSnapshotBuilder::new(generation.get()),
        generation,
        &rows,
        |index| visible.get(index).copied(),
        |row, process| match row {
            ProcessTreeRow::Category { category, .. } => format!("{category:?}"),
            ProcessTreeRow::Application { .. } => process
                .map(|process| process.name.clone())
                .unwrap_or_else(|| String::from("Unknown application")),
            ProcessTreeRow::Process { .. } => process
                .map(|process| process.name.clone())
                .unwrap_or_else(|| String::from("Unknown process")),
        },
        |id| id == ProcessRowId::Process(root_key),
        Some(10_000),
    )
    .build()
    .expect("canonical process semantics must validate");

    let table = snapshot
        .get(&SemanticNodeId::borrowed("process-table"))
        .expect("process table present");
    let child_ids: Vec<_> = table
        .children()
        .skip(3)
        .map(SemanticNodeId::as_str)
        .collect();
    assert_eq!(
        child_ids,
        [
            "group-row:category:Application",
            "group-row:application:pid:10:start:110",
            "row:process:pid:10:start:110",
            "row:process:pid:11:start:111",
            "group-row:category:Uncategorized",
            "row:process:projection:44:row:5:visible:2:unknown",
        ]
    );

    let category = snapshot
        .get(&SemanticNodeId::owned("group-row:category:Application"))
        .expect("application category row present");
    assert_eq!(category.role(), SemanticRole::TreeItem);
    assert_eq!(category.expansion_key(), Some("category:application"));
    assert_eq!(category.state().expanded, Some(true));
    assert!(!category.state().focusable);
    assert!(category.supports_action(SemanticAction::Collapse));
    assert!(!category.supports_action(SemanticAction::Select));
    let category_aggregate = category
        .process_aggregate_semantics()
        .expect("category aggregate stays typed");
    let source_aggregate = rows[0]
        .aggregate()
        .expect("category projection contains its typed source aggregate");
    assert_eq!(category_aggregate.cpu(), source_aggregate.cpu());
    assert_eq!(category_aggregate.memory(), source_aggregate.memory());
    assert_eq!(
        category_aggregate.memory_pss(),
        source_aggregate.memory_pss()
    );
    assert_eq!(category_aggregate.swap(), source_aggregate.swap());
    assert_eq!(category_aggregate.disk_read(), source_aggregate.disk_read());
    assert_eq!(
        category_aggregate.disk_write(),
        source_aggregate.disk_write()
    );
    assert_eq!(category_aggregate.cpu_time(), source_aggregate.cpu_time());
    assert_eq!(category_aggregate.threads(), source_aggregate.threads());
    assert_eq!(category_aggregate.fds(), source_aggregate.fds());
    assert_eq!(category_aggregate.cpu().member_count(), 2);
    assert_eq!(category_aggregate.cpu().current_member_count(), 2);
    assert_eq!(category_aggregate.cpu().known_member_count(), 2);
    assert_eq!(category_aggregate.cpu().current_value(), Some(&53.0));
    assert_eq!(
        category_aggregate.cpu().observation().last_success_ms(),
        Some(10)
    );

    let application = snapshot
        .get(&SemanticNodeId::owned(
            "group-row:application:pid:10:start:110",
        ))
        .expect("application aggregate row present");
    assert_eq!(
        application.expansion_key(),
        Some(app_tree_expansion_key(&items[0]).as_str())
    );
    assert_eq!(application.state().expanded, Some(true));
    assert!(application.supports_action(SemanticAction::Collapse));
    assert!(application.supports_action(SemanticAction::Select));

    let process = snapshot
        .get(&SemanticNodeId::owned("row:process:pid:10:start:110"))
        .expect("process row present");
    assert_eq!(process.role(), SemanticRole::TreeItem);
    assert_eq!(
        process.expansion_key(),
        Some(root_key.stable_key().as_str())
    );
    assert_eq!(process.state().expanded, Some(true));
    assert!(process.supports_action(SemanticAction::Collapse));
    assert!(process.state().selected.unwrap_or_default());
    assert_eq!(
        process
            .process_row_observations()
            .expect("source process facts retained")
            .cpu_percentage()
            .current_value(),
        Some(&3.0)
    );

    let unknown_row = snapshot
        .get(&SemanticNodeId::owned(
            "row:process:projection:44:row:5:visible:2:unknown",
        ))
        .expect("unknown identity row remains visible");
    assert_eq!(unknown_row.state().selected, Some(false));
    assert!(!unknown_row.state().focusable);
    assert!(!unknown_row.supports_action(SemanticAction::Focus));
    assert!(!unknown_row.supports_action(SemanticAction::Select));
    assert_eq!(unknown_row.name(), Some("unknown"));
}

#[test]
fn missing_application_identity_gets_unique_projection_local_non_actionable_ids() {
    let identity = ProcessApplicationIdentity::new("org.example.unknown", "unknown", None)
        .expect("test application identity");
    let item = ProcessItem::new(30, "unknown")
        .with_application_identity_observation(ProcessMetadataObservation::available(identity, 10))
        .with_scalar_observations(ProcessScalarObservations {
            cpu_percentage: ScalarObservation::unavailable(FailureKind::PermissionDenied),
            memory_bytes: ScalarObservation::stale(128, 6, FailureKind::TemporarilyUnavailable),
            ..ProcessScalarObservations::default()
        });
    let expanded = HashSet::from([
        "category:application".to_owned(),
        app_tree_expansion_key(&item),
    ]);
    let visible = [&item];
    let rows = project_process_tree_rows(
        &visible,
        &expanded,
        &HashSet::new(),
        (SortCol::Cpu, SortDir::Desc),
        10,
    );
    assert!(matches!(
        rows.get(1),
        Some(ProcessTreeRow::Application { row_key: None, .. })
    ));
    assert!(matches!(
        rows.get(2),
        Some(ProcessTreeRow::Process { row_key: None, .. })
    ));

    let generation = ProcessProjectionGeneration::new(55);
    let snapshot = append_process_semantic_rows(
        SemanticSnapshotBuilder::new(generation.get()),
        generation,
        &rows,
        |index| visible.get(index).copied(),
        |row, process| match row {
            ProcessTreeRow::Category { category, .. } => format!("{category:?}"),
            ProcessTreeRow::Application { .. } => process
                .map(|process| process.name.clone())
                .unwrap_or_else(|| String::from("Unknown application")),
            ProcessTreeRow::Process { .. } => process
                .map(|process| process.name.clone())
                .unwrap_or_else(|| String::from("Unknown process")),
        },
        |_| true,
        None,
    )
    .build()
    .expect("identity-missing application rows remain valid");

    let application = snapshot
        .get(&SemanticNodeId::owned(
            "group-row:application:projection:55:row:1:visible:0:unknown",
        ))
        .expect("identity-missing application remains visible");
    assert_eq!(application.expansion_key(), Some("app-tree:pid:30:unknown"));
    assert!(!application.state().focusable);
    assert_eq!(application.state().selected, Some(false));
    assert!(!application.supports_action(SemanticAction::Focus));
    assert!(!application.supports_action(SemanticAction::Select));
    assert!(!application.supports_action(SemanticAction::Expand));
    assert!(!application.supports_action(SemanticAction::Collapse));
    let aggregate = application
        .process_aggregate_semantics()
        .expect("application aggregate availability stays typed");
    assert_eq!(
        aggregate.cpu().availability(),
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(aggregate.cpu().current_value(), None);
    assert_eq!(
        aggregate.memory().availability(),
        ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(aggregate.memory().current_value(), None);
    assert_eq!(aggregate.memory().last_known_value(), Some(&128));

    let process = snapshot
        .get(&SemanticNodeId::owned(
            "row:process:projection:55:row:2:visible:0:unknown",
        ))
        .expect("identity-missing process remains visible");
    assert!(!process.state().focusable);
    assert!(!process.supports_action(SemanticAction::Focus));
    assert!(!process.supports_action(SemanticAction::Select));
    let observations = process
        .process_row_observations()
        .expect("source process observations stay typed");
    assert_eq!(
        observations.cpu_percentage().availability(),
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(observations.cpu_percentage().current_value(), None);
    assert_eq!(
        observations.memory_bytes().availability(),
        ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(observations.memory_bytes().current_value(), None);
    assert_eq!(observations.memory_bytes().last_known_value(), Some(&128));
    assert_eq!(
        snapshot
            .get(&SemanticNodeId::owned(
                "row:process:projection:55:row:2:visible:0:unknown:cell:cpu",
            ))
            .and_then(|node| node.value_text()),
        Some("Unavailable")
    );
}
