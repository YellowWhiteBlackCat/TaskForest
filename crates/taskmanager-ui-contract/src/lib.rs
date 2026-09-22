//! Toolkit-neutral semantic contracts shared by graphical and text frontends.

#![forbid(unsafe_code)]

mod accessibility;
mod capabilities;
mod columns;
mod command;
mod conformance;
mod feature_coverage;
mod focus;
mod functional;
mod icon;
mod keybindings;
mod message;
mod navigation;

pub use accessibility::AlertRuleInput;
pub use accessibility::{
    AccessibilityActionRejection, AccessibilityActionRequest, AccessibilityBridge,
    AccessibilityBridgeCapability, AccessibilityBridgeError, AccessibilityBridgeFeatures,
    AccessibilityBridgeStatus, AccessibilityPublication, AccessibilityUnavailableReason,
    DetachedAccessibilityBridge, GraphSummary, ModalInput, ProcessGroupRowInput, ProcessRowInput,
    SemanticAction, SemanticLiveRegion, SemanticNode, SemanticNodeId, SemanticNodeIssue,
    SemanticNumericValue, SemanticRole, SemanticSnapshot, SemanticSnapshotBuilder,
    SemanticSnapshotError, SemanticSort, SemanticState,
};
pub use capabilities::{
    CapabilityCoverageStatus, CapabilityEntry, CapabilityFinding, CapabilityFindingKind,
    CapabilitySemanticSpec, CapabilitySupport, ComponentCapability, FrontendCapabilityDeclaration,
    capability_drift, capability_findings, capability_report,
};
pub use columns::{PROCESS_COLUMNS, ProcessColumnSpec, find};
pub use command::{CommandDescriptor, descriptor};
pub use conformance::ContractTag;
pub use feature_coverage::{
    FeatureArea, FeatureCoverageDeclaration, FeatureCoverageEntry, FeatureCoverageFinding,
    FeatureCoverageFindingKind, FeatureCoverageStatus, FeatureId, FeatureOrigin,
    FeaturePlatformCell, FeaturePlatformLedger, FeaturePlatformStatus, FeatureSemanticSpec,
    MISSING_DECLARATION_REASON, MISSING_UNSUPPORTED_REASON, NO_EVIDENCE, PartialCause,
    PlatformBinding, PlatformCapabilitySurface, PlatformSource, PlatformSourceError,
    PlatformUnavailability, classify, feature_coverage_drift, feature_coverage_findings,
    feature_coverage_report, feature_platform_report,
};
pub use focus::{FocusCycle, FocusCycleStep, FocusRestoreToken, FocusTarget, ModalFocusPolicy};
pub use functional::{
    ContractLayer, FrontendFunctionalDeclaration, FunctionalEntry, FunctionalFinding,
    FunctionalFindingKind, FunctionalStatus, IntentFamily, IntentLifecycle, ProductIntent,
    ProductIntentSpec, SurfaceDecision, TargetContract, functional_drift, functional_findings,
    functional_report,
};
pub use icon::IconId;
pub use keybindings::{
    Binding, BindingCoverageStatus, BindingEntry, FrontendBindingDeclaration, FrontendShape,
    coverage_report, drift_findings,
};
pub use message::MessageKey;
pub use navigation::{
    PageDescriptor, page_descriptor, page_descriptors, page_key_chord, page_shortcut,
};
