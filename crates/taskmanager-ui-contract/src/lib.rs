//! Toolkit-neutral semantic contracts shared by graphical and text frontends.

#![forbid(unsafe_code)]

mod accessibility;
mod columns;
mod command;
mod focus;
mod icon;
mod keybindings;
mod message;
mod navigation;

pub use accessibility::AlertRuleInput;
pub use accessibility::{
    AccessibilityActionRejection, AccessibilityActionRequest, AccessibilityBridge,
    AccessibilityBridgeCapability, AccessibilityBridgeError, AccessibilityBridgeFeatures,
    AccessibilityBridgeStatus, AccessibilityPublication, AccessibilityUnavailableReason,
    DetachedAccessibilityBridge, GraphSummary, ModalInput, ProcessRowInput, SemanticAction,
    SemanticLiveRegion, SemanticNode, SemanticNodeId, SemanticNodeIssue, SemanticNumericValue,
    SemanticRole, SemanticSnapshot, SemanticSnapshotBuilder, SemanticSnapshotError, SemanticSort,
    SemanticState,
};
pub use columns::{PROCESS_COLUMNS, ProcessColumnSpec, find};
pub use command::{CommandDescriptor, descriptor};
pub use focus::{FocusCycle, FocusCycleStep, FocusRestoreToken, FocusTarget, ModalFocusPolicy};
pub use icon::IconId;
pub use keybindings::{
    Binding, BindingEntry, CoverageStatus, FrontendBindingDeclaration, FrontendShape,
    coverage_report, drift_findings,
};
pub use message::MessageKey;
pub use navigation::{
    PageDescriptor, page_descriptor, page_descriptors, page_key_chord, page_shortcut,
};
