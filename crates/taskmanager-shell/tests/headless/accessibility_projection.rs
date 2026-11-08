//! test-intent: behavior

use std::fmt::Write;

use crate::{
    ShellApp, base_semantic_snapshot_builder, demo_app, memory_percentage, semantic_cpu_graph,
    semantic_process_row,
};
use taskmanager_ui_contract::{FrontendShape, SemanticNodeId, SemanticSnapshot};

fn snapshot(shape: FrontendShape, shell: &ShellApp) -> SemanticSnapshot {
    base_semantic_snapshot_builder(shell, shape.name())
        .build()
        .expect("one shared projection must build one validated semantic tree")
}

fn structure(snapshot: &SemanticSnapshot) -> Vec<String> {
    let mut nodes: Vec<_> = snapshot.nodes().collect();
    nodes.sort_by_key(|node| node.id().as_str());
    nodes
        .into_iter()
        .map(|node| {
            let mut line = format!(
                "{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|",
                node.id().as_str(),
                node.role(),
                if node.id().as_str() == "app" {
                    None
                } else {
                    node.name()
                },
                node.value_text(),
                node.numeric_value(),
                node.state(),
                node.live_region(),
                node.actions().collect::<Vec<_>>(),
            );
            for child in node.children() {
                let _ = write!(line, "{};", child.as_str());
            }
            line
        })
        .collect()
}

fn root_name(snapshot: &SemanticSnapshot) -> Option<&str> {
    snapshot.get(&SemanticNodeId::borrowed("app"))?.name()
}

#[test]
fn every_shape_projects_one_byte_identical_semantic_structure() {
    let shell = demo_app();
    let expected = structure(&snapshot(FrontendShape::Gpui, &shell));
    for shape in FrontendShape::ALL {
        let actual = snapshot(shape, &shell);
        assert_eq!(root_name(&actual), Some(shape.name()));
        assert_eq!(
            structure(&actual),
            expected,
            "shape {shape:?} forked the semantic component tree"
        );
    }
}

#[test]
fn owner_preserves_absence_and_bounded_row_facts() {
    let shell = demo_app();
    assert_eq!(memory_percentage(None, Some(100)), None);
    assert_eq!(memory_percentage(Some(10), Some(0)), None);
    assert_eq!(memory_percentage(Some(10), Some(100)), Some(10.0));
    assert!(semantic_cpu_graph(Some(42.0)).is_some());
    assert_eq!(semantic_cpu_graph(None), None);
    assert_eq!(semantic_cpu_graph(Some(f32::NAN)), None);
    let process = shell.visible_processes().into_iter().next();
    let process = process.expect("the deterministic demo projection holds process rows");
    let row = semantic_process_row(process, Some(1024), true);
    assert!(!row.id.is_empty());
    assert!(row.selected);
}

#[test]
fn a_changed_projection_changes_the_structure_signature() {
    let shell = demo_app();
    let before = structure(&snapshot(FrontendShape::Iced, &shell));
    let mut changed = demo_app();
    changed.selected = shell.selected.saturating_add(1);
    let after = structure(&snapshot(FrontendShape::Iced, &changed));
    assert_ne!(
        before, after,
        "the structure signature must be sensitive to projection state"
    );
    assert_eq!(shell.selected, 0);
}
