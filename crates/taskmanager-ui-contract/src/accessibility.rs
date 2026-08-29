//! Toolkit- and operating-system-neutral accessibility contracts.
//!
//! Frontends build a validated [`SemanticSnapshot`] from their localized
//! presentation state. A separately injected [`AccessibilityBridge`] may then
//! publish that snapshot to an operating-system accessibility stack. Defining
//! this model does not imply that such a bridge is linked or usable.

mod bridge;
mod model;
mod snapshot;

pub use bridge::{
    AccessibilityActionRejection, AccessibilityActionRequest, AccessibilityBridge,
    AccessibilityBridgeCapability, AccessibilityBridgeError, AccessibilityBridgeFeatures,
    AccessibilityBridgeStatus, AccessibilityPublication, AccessibilityUnavailableReason,
    DetachedAccessibilityBridge,
};
pub use model::{
    SemanticAction, SemanticLiveRegion, SemanticNode, SemanticNodeId, SemanticNodeIssue,
    SemanticNumericValue, SemanticRole, SemanticSnapshot, SemanticSnapshotError, SemanticSort,
    SemanticState,
};
pub use snapshot::{
    AlertRuleInput, GraphSummary, ModalInput, ProcessGroupRowInput, ProcessRowInput,
    SemanticSnapshotBuilder,
};

#[cfg(test)]
#[path = "../tests/headless/ui_accessibility.rs"]
mod tests;
