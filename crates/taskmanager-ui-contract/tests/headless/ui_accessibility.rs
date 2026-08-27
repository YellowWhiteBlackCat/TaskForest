use std::collections::VecDeque;
use std::sync::Mutex;

use super::*;

const ROOT: SemanticNodeId = SemanticNodeId::borrowed("app");
const SWITCH: SemanticNodeId = SemanticNodeId::borrowed("pause-switch");
const TABLE: SemanticNodeId = SemanticNodeId::borrowed("process-table");
const GRAPH: SemanticNodeId = SemanticNodeId::borrowed("cpu-graph");
const DIALOG: SemanticNodeId = SemanticNodeId::borrowed("kill-dialog");
const FEEDBACK: SemanticNodeId = SemanticNodeId::borrowed("feedback");

fn representative_snapshot(revision: u64) -> SemanticSnapshot {
    let switch = SemanticNode::new(SWITCH.clone(), SemanticRole::Switch)
        .named("Pause updates")
        .with_state(SemanticState {
            focusable: true,
            checked: Some(false),
            ..SemanticState::default()
        })
        .with_action(SemanticAction::Focus)
        .with_action(SemanticAction::Toggle);
    let table = SemanticNode::new(TABLE.clone(), SemanticRole::Table)
        .named("Processes")
        .described("Sortable process list");
    let graph = SemanticNode::new(GRAPH.clone(), SemanticRole::Graph)
        .named("CPU history")
        .with_value_text("Latest 18%, peak 72%")
        .with_numeric_value(SemanticNumericValue {
            current: 18.0,
            minimum: 0.0,
            maximum: 100.0,
        })
        .with_state(SemanticState {
            focusable: true,
            ..SemanticState::default()
        })
        .with_action(SemanticAction::Focus)
        .with_action(SemanticAction::ReadPreviousValue)
        .with_action(SemanticAction::ReadNextValue);
    let dialog = SemanticNode::new(DIALOG.clone(), SemanticRole::AlertDialog)
        .named("End process?")
        .with_state(SemanticState {
            modal: true,
            ..SemanticState::default()
        })
        .with_action(SemanticAction::Dismiss);
    let feedback = SemanticNode::new(FEEDBACK.clone(), SemanticRole::StaticText)
        .named("Process ended")
        .with_live_region(SemanticLiveRegion::Polite);
    let root = SemanticNode::new(ROOT.clone(), SemanticRole::Application).with_children([
        SWITCH.clone(),
        TABLE.clone(),
        GRAPH.clone(),
        DIALOG.clone(),
        FEEDBACK.clone(),
    ]);
    SemanticSnapshot::new(
        revision,
        ROOT.clone(),
        [root, switch, table, graph, dialog, feedback],
    )
    .expect("representative accessibility tree should be valid")
}

#[test]
fn snapshot_covers_interaction_table_graph_dialog_and_live_feedback_semantics() {
    let snapshot = representative_snapshot(41);

    assert_eq!(snapshot.revision(), 41);
    assert_eq!(snapshot.root(), &ROOT);
    assert_eq!(snapshot.nodes().count(), 6);
    assert_eq!(
        snapshot.get(&SWITCH).map(SemanticNode::state),
        Some(SemanticState {
            focusable: true,
            checked: Some(false),
            ..SemanticState::default()
        })
    );
    assert_eq!(
        snapshot.get(&GRAPH).and_then(SemanticNode::value_text),
        Some("Latest 18%, peak 72%")
    );
    assert_eq!(
        snapshot.get(&FEEDBACK).map(SemanticNode::live_region),
        Some(SemanticLiveRegion::Polite)
    );
}

#[test]
fn invalid_trees_and_untruthful_node_state_fail_closed() {
    let unnamed_switch = SemanticNode::new(SWITCH.clone(), SemanticRole::Switch);
    let root =
        SemanticNode::new(ROOT.clone(), SemanticRole::Application).with_child(SWITCH.clone());
    assert_eq!(
        SemanticSnapshot::new(1, ROOT.clone(), [root.clone(), unnamed_switch]),
        Err(SemanticSnapshotError::InvalidNode {
            node: SWITCH.clone(),
            issue: SemanticNodeIssue::MissingInteractiveName,
        })
    );

    let actionable_disabled = SemanticNode::new(SWITCH.clone(), SemanticRole::Switch)
        .named("Pause")
        .with_state(SemanticState {
            disabled: true,
            checked: Some(false),
            ..SemanticState::default()
        })
        .with_action(SemanticAction::Toggle);
    assert_eq!(
        SemanticSnapshot::new(1, ROOT.clone(), [root, actionable_disabled]),
        Err(SemanticSnapshotError::InvalidNode {
            node: SWITCH.clone(),
            issue: SemanticNodeIssue::DisabledHasActions,
        })
    );

    let root =
        SemanticNode::new(ROOT.clone(), SemanticRole::Application).with_child(SWITCH.clone());
    let child = SemanticNode::new(SWITCH.clone(), SemanticRole::Group).with_child(ROOT.clone());
    assert_eq!(
        SemanticSnapshot::new(1, ROOT.clone(), [root, child]),
        Err(SemanticSnapshotError::Cycle(ROOT.clone()))
    );
}

#[test]
fn structural_tree_errors_are_rejected_before_publication() {
    let root =
        SemanticNode::new(ROOT.clone(), SemanticRole::Application).with_child(SWITCH.clone());
    assert_eq!(
        SemanticSnapshot::new(1, ROOT.clone(), [root.clone()]),
        Err(SemanticSnapshotError::MissingChild {
            parent: ROOT.clone(),
            child: SWITCH.clone(),
        })
    );

    let duplicate_child = SemanticNode::new(ROOT.clone(), SemanticRole::Application)
        .with_children([SWITCH.clone(), SWITCH.clone()]);
    let child = SemanticNode::new(SWITCH.clone(), SemanticRole::Group);
    assert_eq!(
        SemanticSnapshot::new(1, ROOT.clone(), [duplicate_child, child.clone()]),
        Err(SemanticSnapshotError::InvalidNode {
            node: ROOT.clone(),
            issue: SemanticNodeIssue::DuplicateChild,
        })
    );

    let other_parent = SemanticNodeId::borrowed("other-parent");
    let root = root.with_child(other_parent.clone());
    let parent =
        SemanticNode::new(other_parent.clone(), SemanticRole::Group).with_child(SWITCH.clone());
    assert_eq!(
        SemanticSnapshot::new(1, ROOT.clone(), [root, parent, child.clone()]),
        Err(SemanticSnapshotError::MultipleParents(SWITCH.clone()))
    );

    let root = SemanticNode::new(ROOT.clone(), SemanticRole::Application);
    assert_eq!(
        SemanticSnapshot::new(1, ROOT.clone(), [root, child]),
        Err(SemanticSnapshotError::Disconnected(SWITCH.clone()))
    );
    assert_eq!(
        SemanticSnapshot::new(
            1,
            SWITCH.clone(),
            [SemanticNode::new(SWITCH.clone(), SemanticRole::Group)]
        ),
        Err(SemanticSnapshotError::InvalidRootRole(SWITCH.clone()))
    );
}

#[test]
fn invalid_role_state_action_and_numeric_combinations_are_rejected() {
    let invalid_nodes = [
        (
            SemanticNode::new(SWITCH.clone(), SemanticRole::Switch)
                .named("Pause")
                .with_state(SemanticState {
                    focused: true,
                    checked: Some(false),
                    ..SemanticState::default()
                }),
            SemanticNodeIssue::FocusedWithoutFocusable,
        ),
        (
            SemanticNode::new(SWITCH.clone(), SemanticRole::Switch)
                .named("Pause")
                .with_state(SemanticState {
                    focusable: true,
                    checked: Some(false),
                    ..SemanticState::default()
                })
                .with_action(SemanticAction::Press),
            SemanticNodeIssue::UnsupportedActionForRole,
        ),
        (
            SemanticNode::new(SWITCH.clone(), SemanticRole::Switch)
                .named("Pause")
                .with_state(SemanticState {
                    focusable: true,
                    checked: Some(false),
                    sort: Some(SemanticSort::Ascending),
                    ..SemanticState::default()
                }),
            SemanticNodeIssue::SortOnUnsupportedRole,
        ),
        (
            SemanticNode::new(SWITCH.clone(), SemanticRole::Switch)
                .named("Pause")
                .with_numeric_value(SemanticNumericValue {
                    current: 101.0,
                    minimum: 0.0,
                    maximum: 100.0,
                }),
            SemanticNodeIssue::NumericValueOnUnsupportedRole,
        ),
        (
            SemanticNode::new(SWITCH.clone(), SemanticRole::Slider)
                .named("Refresh interval")
                .with_numeric_value(SemanticNumericValue {
                    current: f64::NAN,
                    minimum: 0.5,
                    maximum: 5.0,
                }),
            SemanticNodeIssue::InvalidNumericRange,
        ),
    ];

    for (node, issue) in invalid_nodes {
        let root =
            SemanticNode::new(ROOT.clone(), SemanticRole::Application).with_child(SWITCH.clone());
        assert_eq!(
            SemanticSnapshot::new(1, ROOT.clone(), [root, node]),
            Err(SemanticSnapshotError::InvalidNode {
                node: SWITCH.clone(),
                issue,
            })
        );
    }
}

#[test]
fn detached_bridge_never_mistakes_a_semantic_model_for_native_support() {
    let bridge = DetachedAccessibilityBridge;

    assert_eq!(
        bridge.capability(),
        AccessibilityBridgeCapability::backend_not_linked()
    );
    assert!(!bridge.capability().is_ready());
    assert_eq!(
        bridge.try_publish(representative_snapshot(1)),
        Err(AccessibilityBridgeError::BackendNotLinked)
    );
    assert_eq!(
        bridge.try_recv_action(),
        Err(AccessibilityBridgeError::BackendNotLinked)
    );
}

#[derive(Default)]
struct RecordingBridge {
    published: Mutex<Vec<u64>>,
    actions: Mutex<VecDeque<AccessibilityActionRequest>>,
}

impl AccessibilityBridge for RecordingBridge {
    fn capability(&self) -> AccessibilityBridgeCapability {
        AccessibilityBridgeCapability::ready(AccessibilityBridgeFeatures {
            actions: true,
            live_regions: true,
            tables: true,
            graph_navigation: true,
        })
    }

    fn try_publish(
        &self,
        snapshot: SemanticSnapshot,
    ) -> Result<AccessibilityPublication, AccessibilityBridgeError> {
        let revision = snapshot.revision();
        self.published.lock().expect("test mutex").push(revision);
        Ok(AccessibilityPublication {
            snapshot_revision: revision,
        })
    }

    fn try_recv_action(
        &self,
    ) -> Result<Option<AccessibilityActionRequest>, AccessibilityBridgeError> {
        Ok(self.actions.lock().expect("test mutex").pop_front())
    }
}

#[test]
fn frontend_can_replace_the_backend_without_changing_snapshot_vocabulary() {
    let bridge: Box<dyn AccessibilityBridge> = Box::new(RecordingBridge::default());
    assert!(bridge.capability().is_ready());

    assert_eq!(
        bridge.try_publish(representative_snapshot(77)),
        Ok(AccessibilityPublication {
            snapshot_revision: 77,
        })
    );
    assert_eq!(bridge.try_recv_action(), Ok(None));
}

#[test]
fn native_actions_are_bound_to_revision_identity_state_and_declared_action() {
    let snapshot = representative_snapshot(77);
    let valid = AccessibilityActionRequest {
        snapshot_revision: 77,
        node: SWITCH.clone(),
        action: SemanticAction::Toggle,
        value: None,
    };
    assert_eq!(valid.validate_against(&snapshot), Ok(()));

    assert_eq!(
        AccessibilityActionRequest {
            snapshot_revision: 76,
            ..valid.clone()
        }
        .validate_against(&snapshot),
        Err(AccessibilityActionRejection::StaleSnapshot {
            current: 77,
            requested: 76,
        })
    );
    assert_eq!(
        AccessibilityActionRequest {
            action: SemanticAction::SetValue,
            ..valid
        }
        .validate_against(&snapshot),
        Err(AccessibilityActionRejection::UnsupportedAction)
    );

    assert_eq!(
        AccessibilityActionRequest {
            snapshot_revision: 77,
            node: SemanticNodeId::borrowed("missing"),
            action: SemanticAction::Press,
            value: None,
        }
        .validate_against(&snapshot),
        Err(AccessibilityActionRejection::UnknownNode)
    );
}
