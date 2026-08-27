//! source-inspection: static-policy
//!
//! Architecture guard for the toolkit-neutral accessibility seam.

use std::fs;
use std::path::Path;

use taskmanager_ui_contract::{
    AccessibilityBridge, AccessibilityBridgeError, AccessibilityBridgeStatus,
    DetachedAccessibilityBridge, SemanticNode, SemanticNodeId, SemanticRole, SemanticSnapshot,
};

#[test]
fn accessibility_contract_remains_free_of_toolkit_and_native_api_vocabulary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/taskmanager-ui-contract/src");
    let mut pending = vec![root];
    let mut sources = String::new();
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).expect("UI contract source should be readable") {
            let path = entry.expect("UI contract entry should be readable").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                sources.push_str(
                    &fs::read_to_string(path).expect("UI contract module should be readable"),
                );
            }
        }
    }

    for forbidden in [
        "gpui::",
        "accesskit",
        "AccessKit",
        "at_spi",
        "AT-SPI",
        "UIAutomation",
        "NSAccessibility",
    ] {
        assert!(
            !sources.contains(forbidden),
            "shared accessibility contract leaked outer adapter vocabulary: {forbidden}"
        );
    }
}

#[test]
fn detached_frontend_cannot_claim_native_semantic_tree_support() {
    let bridge = DetachedAccessibilityBridge;
    assert_eq!(
        bridge.capability().status(),
        AccessibilityBridgeStatus::BackendNotLinked
    );

    let root = SemanticNodeId::borrowed("root");
    let snapshot = SemanticSnapshot::new(
        1,
        root.clone(),
        [SemanticNode::new(root, SemanticRole::Application)],
    )
    .expect("minimal semantic snapshot should validate");
    assert_eq!(
        bridge.try_publish(snapshot),
        Err(AccessibilityBridgeError::BackendNotLinked)
    );
}
