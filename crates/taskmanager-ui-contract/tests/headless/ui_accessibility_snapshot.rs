use super::*;
use crate::{
    AccessibilityBridge, AccessibilityBridgeCapability, AccessibilityBridgeError,
    DetachedAccessibilityBridge, ProcessAggregateSemantics, ProcessRowBehavior,
    ProcessRowObservations, SemanticNodeIssue, SemanticSnapshotError,
};
use taskmanager_core::core::process::aggregate::{aggregate_f32, aggregate_u64};
use taskmanager_core::core::{FailureKind, ScalarAvailability, ScalarObservation};

fn sample_rows() -> [ProcessRowInput; 2] {
    [
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
    ]
}

#[test]
fn builder_produces_well_formed_table_graph_and_live_region_tree() {
    let snapshot = SemanticSnapshotBuilder::new(7)
        .application_name("TaskForest")
        .process_rows(sample_rows())
        .cpu_graph(GraphSummary {
            current: 18.0,
            peak: 72.0,
            maximum: 100.0,
        })
        .status_announcement("2 processes visible")
        .build()
        .expect("canonical builder tree must be well-formed");

    assert_eq!(snapshot.revision(), 7);
    // root + main + table + 3 headers + (2 rows * 4 nodes) + graph + status = 16
    assert_eq!(snapshot.nodes().count(), 16);
    assert_eq!(snapshot.root().as_str(), "app");

    let graph = snapshot
        .get(&SemanticNodeId::borrowed(GRAPH_ID))
        .expect("cpu-graph node present");
    assert_eq!(graph.value_text(), Some("Latest 18%, peak 72%"));
    let numeric = graph.numeric_value().expect("graph has numeric value");
    assert_eq!(numeric.current, 18.0);
    assert_eq!(numeric.minimum, 0.0);
    assert_eq!(numeric.maximum, 100.0);
    assert!(graph.supports_action(SemanticAction::ReadNextValue));

    let selected_row = snapshot
        .get(&SemanticNodeId::owned("row:1024"))
        .expect("firefox row present");
    assert_eq!(selected_row.state().selected, Some(true));
    assert!(selected_row.supports_action(SemanticAction::Select));

    let unselected_row = snapshot
        .get(&SemanticNodeId::owned("row:2048"))
        .expect("cargo row present");
    assert_eq!(unselected_row.state().selected, Some(false));

    let cpu_cell = snapshot
        .get(&SemanticNodeId::owned("row:2048:cell:cpu"))
        .expect("cargo cpu cell present");
    assert_eq!(cpu_cell.value_text(), Some("87.6%"));

    let status = snapshot
        .get(&SemanticNodeId::borrowed(STATUS_ID))
        .expect("status node present");
    assert_eq!(status.live_region(), SemanticLiveRegion::Polite);
}

#[test]
fn builder_minimal_tree_without_graph_or_status_still_validates() {
    let snapshot = SemanticSnapshotBuilder::new(3)
        .process_row(sample_rows()[0].clone())
        .build()
        .expect("minimal tree must be well-formed");
    // root + main + table + 3 headers + 1 row + 3 cells = 10
    assert_eq!(snapshot.nodes().count(), 10);
    assert!(snapshot.get(&SemanticNodeId::borrowed(GRAPH_ID)).is_none());
    assert!(snapshot.get(&SemanticNodeId::borrowed(STATUS_ID)).is_none());
}

#[test]
fn structural_process_group_rows_publish_tree_state_without_fake_metrics() {
    let snapshot = SemanticSnapshotBuilder::new(8)
        .process_group_row(ProcessGroupRowInput {
            id: String::from("category:application"),
            name: String::from("Applications (2)"),
            expanded: true,
            selected: true,
        })
        .process_row(sample_rows()[0].clone())
        .build()
        .expect("structural row must validate");

    let group = snapshot
        .get(&SemanticNodeId::owned("group-row:category:application"))
        .expect("group TreeItem present");
    assert_eq!(group.role(), SemanticRole::TreeItem);
    assert_eq!(group.name(), Some("Applications (2)"));
    assert_eq!(group.state().selected, Some(true));
    assert_eq!(group.state().expanded, Some(true));
    assert!(group.supports_action(SemanticAction::Focus));
    assert!(group.supports_action(SemanticAction::Select));
    assert!(group.supports_action(SemanticAction::Collapse));
    assert!(!group.supports_action(SemanticAction::Expand));
}

#[test]
fn typed_process_row_observations_preserve_current_and_retained_truth() {
    let cpu = ScalarObservation::partial(0.0, 41, FailureKind::PermissionDenied);
    let memory = ScalarObservation::stale(0, 39, FailureKind::TemporarilyUnavailable);
    let snapshot = SemanticSnapshotBuilder::new(9)
        .process_row_with_semantics(
            ProcessRowInput {
                id: String::from("typed-process"),
                name: String::from("typed process"),
                cpu_percent: Some(0.0),
                memory_percent: None,
                selected: true,
            },
            ProcessRowBehavior::NonActionable,
            Some(ProcessRowObservations::new(cpu, memory)),
        )
        .build()
        .expect("typed process observations must validate");

    let row = snapshot
        .get(&SemanticNodeId::owned("row:typed-process"))
        .expect("typed process row is retained");
    let observations = row
        .process_row_observations()
        .expect("typed observations stay on the semantic row");
    assert_eq!(observations.cpu_percentage(), &cpu);
    assert_eq!(observations.memory_bytes(), &memory);
    assert_eq!(
        observations.cpu_percentage().availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied)
    );
    assert_eq!(observations.cpu_percentage().current_value(), Some(&0.0));
    assert_eq!(observations.cpu_percentage().last_success_ms(), Some(41));
    assert_eq!(
        observations.memory_bytes().availability(),
        ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(observations.memory_bytes().current_value(), None);
    assert_eq!(observations.memory_bytes().last_known_value(), Some(&0));
    assert_eq!(observations.memory_bytes().last_success_ms(), Some(39));
    assert_eq!(row.state().selected, Some(false));
    assert!(!row.supports_action(SemanticAction::Focus));
    assert!(!row.supports_action(SemanticAction::Select));
}

#[test]
fn typed_aggregate_semantics_keep_availability_coverage_and_observation() {
    let cases = [
        (
            "zero",
            vec![ScalarObservation::available(0.0_f32, 7)],
            ScalarAvailability::Available,
            Some(0.0),
            Some(0.0),
            1,
            1,
            1,
            Some(999),
        ),
        (
            "partial",
            vec![
                ScalarObservation::available(2.0_f32, 10),
                ScalarObservation::default(),
            ],
            ScalarAvailability::Partial(FailureKind::TemporarilyUnavailable),
            Some(2.0),
            Some(2.0),
            2,
            1,
            1,
            Some(999),
        ),
        (
            "stale",
            vec![ScalarObservation::stale(
                4.0_f32,
                8,
                FailureKind::TemporarilyUnavailable,
            )],
            ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable),
            None,
            Some(4.0),
            1,
            0,
            1,
            Some(8),
        ),
        (
            "unknown",
            vec![ScalarObservation::default()],
            ScalarAvailability::Unknown,
            None,
            None,
            1,
            0,
            0,
            None,
        ),
        (
            "permission denied",
            vec![ScalarObservation::unavailable(
                FailureKind::PermissionDenied,
            )],
            ScalarAvailability::Unavailable(FailureKind::PermissionDenied),
            None,
            None,
            1,
            0,
            0,
            None,
        ),
        (
            "unavailable",
            vec![ScalarObservation::unavailable(
                FailureKind::TemporarilyUnavailable,
            )],
            ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable),
            None,
            None,
            1,
            0,
            0,
            None,
        ),
    ];

    for (
        name,
        observations,
        availability,
        current,
        last_known,
        members,
        current_members,
        known_members,
        last_success,
    ) in cases
    {
        let cpu = aggregate_f32(observations.iter(), 999).expect("one aggregate member");
        let expected_cpu = cpu.clone();
        let memory_observations: Vec<_> = observations
            .iter()
            .map(|observation| match observation.availability() {
                ScalarAvailability::Available => ScalarObservation::available(
                    observation.current_value().copied().unwrap_or_default() as u64,
                    observation.last_success_ms().unwrap_or_default(),
                ),
                ScalarAvailability::Partial(failure) => ScalarObservation::partial(
                    observation.current_value().copied().unwrap_or_default() as u64,
                    observation.last_success_ms().unwrap_or_default(),
                    failure,
                ),
                ScalarAvailability::Stale(failure) => ScalarObservation::stale(
                    observation.last_known_value().copied().unwrap_or_default() as u64,
                    observation.last_success_ms().unwrap_or_default(),
                    failure,
                ),
                ScalarAvailability::Unavailable(failure) => ScalarObservation::unavailable(failure),
                ScalarAvailability::Unknown => ScalarObservation::default(),
            })
            .collect();
        let memory = aggregate_u64(memory_observations.iter(), 999).expect("one aggregate member");
        let expected_memory = memory.clone();
        let semantics = ProcessAggregateSemantics {
            cpu,
            memory: memory.clone(),
            memory_pss: memory.clone(),
            swap: memory.clone(),
            disk_read: memory.clone(),
            disk_write: memory.clone(),
            cpu_time: memory.clone(),
            threads: memory.clone(),
            fds: memory,
        };
        let snapshot = SemanticSnapshotBuilder::new(10)
            .process_group_row_with_semantics(
                ProcessGroupRowInput {
                    id: format!("aggregate-{name}"),
                    name: format!("aggregate {name}"),
                    expanded: false,
                    selected: true,
                },
                Some(String::from("group-expansion")),
                true,
                true,
                semantics,
            )
            .build()
            .expect("typed aggregate group must validate");
        let node = snapshot
            .get(&SemanticNodeId::owned(format!(
                "group-row:aggregate-{name}"
            )))
            .expect("aggregate semantic row is retained");
        let semantic = node
            .process_aggregate_semantics()
            .expect("aggregate values stay attached to the row");
        let cpu = semantic.cpu();
        assert_eq!(cpu.availability(), availability, "{name}");
        assert_eq!(cpu.current_value().copied(), current, "{name}");
        assert_eq!(cpu.last_known_value().copied(), last_known, "{name}");
        assert_eq!(cpu.observation().last_success_ms(), last_success, "{name}");
        assert_eq!(cpu.member_count(), members, "{name}");
        assert_eq!(cpu.current_member_count(), current_members, "{name}");
        assert_eq!(cpu.known_member_count(), known_members, "{name}");
        assert_eq!(semantic.memory(), &expected_memory, "{name}");
        assert_eq!(semantic.memory_pss(), &expected_memory, "{name}");
        assert_eq!(semantic.swap(), &expected_memory, "{name}");
        assert_eq!(semantic.disk_read(), &expected_memory, "{name}");
        assert_eq!(semantic.disk_write(), &expected_memory, "{name}");
        assert_eq!(semantic.cpu_time(), &expected_memory, "{name}");
        assert_eq!(semantic.threads(), &expected_memory, "{name}");
        assert_eq!(semantic.fds(), &expected_memory, "{name}");
        assert_eq!(expected_cpu.member_count(), members, "{name}");
    }
}

#[test]
fn unavailable_process_scalars_are_spoken_as_unavailable_not_zero() {
    let snapshot = SemanticSnapshotBuilder::new(4)
        .process_row(ProcessRowInput {
            id: String::from("9"),
            name: String::from("unknown"),
            cpu_percent: None,
            memory_percent: Some(0.0),
            selected: false,
        })
        .build()
        .expect("unavailable process scalars must remain a valid tree");

    assert_eq!(
        snapshot
            .get(&SemanticNodeId::owned("row:9:cell:cpu"))
            .and_then(|node| node.value_text()),
        Some("Unavailable")
    );
    assert_eq!(
        snapshot
            .get(&SemanticNodeId::owned("row:9:cell:memory"))
            .and_then(|node| node.value_text()),
        Some("0.0%")
    );
}

#[test]
fn modal_input_creates_a_dismissible_modal_semantic_node() {
    let snapshot = SemanticSnapshotBuilder::new(5)
        .modal(ModalInput {
            id: String::from("keyboard-help"),
            name: String::from("Keyboard help"),
            description: Some(String::from("Shared command vocabulary")),
        })
        .build()
        .expect("modal semantic node must validate");
    let modal = snapshot
        .get(&SemanticNodeId::owned("modal:keyboard-help"))
        .expect("modal node present");

    assert_eq!(modal.role(), SemanticRole::Dialog);
    assert!(modal.state().modal);
    assert!(modal.state().focusable);
    assert!(modal.supports_action(SemanticAction::Dismiss));
}

#[test]
fn alert_rules_publish_a_named_group_of_toggleable_switches() {
    let snapshot = SemanticSnapshotBuilder::new(6)
        .alert_rules(
            "Alert rules",
            [
                AlertRuleInput {
                    id: String::from("cpu-high"),
                    name: String::from("CPU usage"),
                    enabled: true,
                    detail: Some(String::from("Warning · ≥ 90.0% · triggered")),
                },
                AlertRuleInput {
                    id: String::from("mem-high"),
                    name: String::from("Memory usage"),
                    enabled: false,
                    detail: None,
                },
            ],
        )
        .build()
        .expect("alerts group must validate");

    let group = snapshot
        .get(&SemanticNodeId::borrowed(ALERTS_ID))
        .expect("alerts group present");
    assert_eq!(group.role(), SemanticRole::Group);
    assert_eq!(group.name(), Some("Alert rules"));
    assert_eq!(group.children().count(), 2);

    let enabled = snapshot
        .get(&SemanticNodeId::owned("alert-rule:cpu-high"))
        .expect("enabled rule switch present");
    assert_eq!(enabled.role(), SemanticRole::Switch);
    assert_eq!(enabled.state().checked, Some(true));
    assert!(enabled.supports_action(SemanticAction::Toggle));
    assert_eq!(enabled.description(), Some("Warning · ≥ 90.0% · triggered"));

    let disabled = snapshot
        .get(&SemanticNodeId::owned("alert-rule:mem-high"))
        .expect("disabled rule switch present");
    assert_eq!(disabled.state().checked, Some(false));
    assert_eq!(disabled.description(), None);
}

#[test]
fn builder_fails_closed_when_graph_range_overflows() {
    let err = SemanticSnapshotBuilder::new(1)
        .cpu_graph(GraphSummary {
            current: 101.0,
            peak: 105.0,
            maximum: 100.0,
        })
        .build()
        .expect_err("overflowed graph range must be rejected");
    assert!(matches!(
        err,
        SemanticSnapshotError::InvalidNode {
            issue: SemanticNodeIssue::InvalidNumericRange,
            ..
        }
    ));
}

#[test]
fn detached_bridge_round_trips_builder_snapshot_without_ever_claiming_support() {
    // The contract side is real: a well-formed snapshot exists and is
    // ready for an adapter to consume. The detached bridge must reject it
    // honestly rather than pretending to publish.
    let snapshot = SemanticSnapshotBuilder::new(99)
        .process_rows(sample_rows())
        .cpu_graph(GraphSummary {
            current: 18.0,
            peak: 72.0,
            maximum: 100.0,
        })
        .status_announcement("Snapshot ready for publication")
        .build()
        .expect("builder snapshot is well-formed");

    let revision = snapshot.revision();
    let node_count = snapshot.nodes().count();

    let bridge = DetachedAccessibilityBridge;
    assert_eq!(
        bridge.capability(),
        AccessibilityBridgeCapability::backend_not_linked(),
    );
    assert!(!bridge.capability().is_ready());
    assert_eq!(
        bridge.try_publish(snapshot.clone()),
        Err(AccessibilityBridgeError::BackendNotLinked),
    );
    assert_eq!(
        bridge.try_recv_action(),
        Err(AccessibilityBridgeError::BackendNotLinked),
    );

    // The snapshot survives the rejection unchanged for a real adapter.
    assert_eq!(snapshot.revision(), revision);
    assert_eq!(snapshot.nodes().count(), node_count);
    assert!(snapshot.get(&SemanticNodeId::borrowed(GRAPH_ID)).is_some());
}
