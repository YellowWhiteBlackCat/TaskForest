//! Replaceable accessibility bridge trait and its truthful runtime
//! capability, status, publication, action-request, and error types.
//!
//! Bridge methods must not perform blocking native I/O on the render thread,
//! and a bare bus/session marker must never make capability report `Ready`.

use super::{SemanticAction, SemanticNodeId, SemanticSnapshot};

/// Frontend-neutral reason a linked bridge cannot currently publish.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccessibilityUnavailableReason {
    NoAssistiveTechnologySession,
    PermissionDenied,
    InitializationFailed,
    RuntimeStopped,
}

/// Truthful runtime state of the accessibility bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccessibilityBridgeStatus {
    /// No toolkit/OS bridge implementation is linked into this frontend.
    BackendNotLinked,
    /// This frontend or target cannot provide an accessibility bridge.
    Unsupported,
    /// A bridge is linked, but is not currently usable.
    Unavailable(AccessibilityUnavailableReason),
    /// The bridge has initialized and can accept semantic snapshots.
    Ready,
}

/// Optional semantics implemented by a ready bridge. A ready bridge always
/// includes the base semantic tree; these flags describe deeper behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AccessibilityBridgeFeatures {
    pub actions: bool,
    pub live_regions: bool,
    pub tables: bool,
    pub graph_navigation: bool,
}

/// Runtime capability receipt returned by an injected bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AccessibilityBridgeCapability {
    status: AccessibilityBridgeStatus,
    features: AccessibilityBridgeFeatures,
}

impl AccessibilityBridgeCapability {
    #[must_use]
    pub const fn backend_not_linked() -> Self {
        Self {
            status: AccessibilityBridgeStatus::BackendNotLinked,
            features: AccessibilityBridgeFeatures {
                actions: false,
                live_regions: false,
                tables: false,
                graph_navigation: false,
            },
        }
    }

    #[must_use]
    pub const fn unsupported() -> Self {
        Self {
            status: AccessibilityBridgeStatus::Unsupported,
            features: AccessibilityBridgeFeatures {
                actions: false,
                live_regions: false,
                tables: false,
                graph_navigation: false,
            },
        }
    }

    #[must_use]
    pub const fn unavailable(reason: AccessibilityUnavailableReason) -> Self {
        Self {
            status: AccessibilityBridgeStatus::Unavailable(reason),
            features: AccessibilityBridgeFeatures {
                actions: false,
                live_regions: false,
                tables: false,
                graph_navigation: false,
            },
        }
    }

    #[must_use]
    pub const fn ready(features: AccessibilityBridgeFeatures) -> Self {
        Self {
            status: AccessibilityBridgeStatus::Ready,
            features,
        }
    }

    #[must_use]
    pub const fn status(self) -> AccessibilityBridgeStatus {
        self.status
    }

    #[must_use]
    pub const fn features(self) -> AccessibilityBridgeFeatures {
        self.features
    }

    #[must_use]
    pub const fn is_ready(self) -> bool {
        matches!(self.status, AccessibilityBridgeStatus::Ready)
    }
}

/// An action emitted by the native accessibility stack against one published
/// revision. Frontends must reject requests for stale node identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessibilityActionRequest {
    pub snapshot_revision: u64,
    pub node: SemanticNodeId,
    pub action: SemanticAction,
    pub value: Option<String>,
}

impl AccessibilityActionRequest {
    /// Validate a native action against the exact published snapshot before
    /// dispatching it into application behavior.
    pub fn validate_against(
        &self,
        snapshot: &SemanticSnapshot,
    ) -> Result<(), AccessibilityActionRejection> {
        if self.snapshot_revision != snapshot.revision() {
            return Err(AccessibilityActionRejection::StaleSnapshot {
                current: snapshot.revision(),
                requested: self.snapshot_revision,
            });
        }
        let Some(node) = snapshot.get(&self.node) else {
            return Err(AccessibilityActionRejection::UnknownNode);
        };
        if node.state().disabled {
            return Err(AccessibilityActionRejection::DisabledNode);
        }
        if !node.supports_action(self.action) {
            return Err(AccessibilityActionRejection::UnsupportedAction);
        }
        match (self.action, self.value.as_deref()) {
            (SemanticAction::SetValue, Some(value)) if !value.trim().is_empty() => Ok(()),
            (SemanticAction::SetValue, _) => Err(AccessibilityActionRejection::MissingValue),
            (_, None) => Ok(()),
            (_, Some(_)) => Err(AccessibilityActionRejection::UnexpectedValue),
        }
    }
}

/// Typed rejection of an assistive-technology action before application dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccessibilityActionRejection {
    StaleSnapshot { current: u64, requested: u64 },
    UnknownNode,
    DisabledNode,
    UnsupportedAction,
    MissingValue,
    UnexpectedValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AccessibilityPublication {
    pub snapshot_revision: u64,
}

/// Non-blocking bridge errors. Native diagnostic text remains adapter-owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccessibilityBridgeError {
    BackendNotLinked,
    Unsupported,
    Unavailable(AccessibilityUnavailableReason),
    Backpressure,
    StaleRevision { current: u64, submitted: u64 },
}

/// Replaceable publishing and action port at the frontend composition edge.
///
/// Implementations may enqueue work, but these methods must not perform
/// blocking native I/O on the render thread. A bus/session marker alone must
/// never make [`Self::capability`] return `Ready`.
pub trait AccessibilityBridge: Send + Sync {
    fn capability(&self) -> AccessibilityBridgeCapability;

    fn try_publish(
        &self,
        snapshot: SemanticSnapshot,
    ) -> Result<AccessibilityPublication, AccessibilityBridgeError>;

    fn try_recv_action(
        &self,
    ) -> Result<Option<AccessibilityActionRequest>, AccessibilityBridgeError>;
}

/// Honest default for frontends whose toolkit has no semantic bridge.
#[derive(Clone, Copy, Debug, Default)]
pub struct DetachedAccessibilityBridge;

impl AccessibilityBridge for DetachedAccessibilityBridge {
    fn capability(&self) -> AccessibilityBridgeCapability {
        AccessibilityBridgeCapability::backend_not_linked()
    }

    fn try_publish(
        &self,
        _snapshot: SemanticSnapshot,
    ) -> Result<AccessibilityPublication, AccessibilityBridgeError> {
        Err(AccessibilityBridgeError::BackendNotLinked)
    }

    fn try_recv_action(
        &self,
    ) -> Result<Option<AccessibilityActionRequest>, AccessibilityBridgeError> {
        Err(AccessibilityBridgeError::BackendNotLinked)
    }
}
