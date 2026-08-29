//! Provider, operation, submission, and event-port failure contracts with
//! retry disposition.
//!
//! Provider failures precede request correlation; operation and delivery
//! failures are tracked separately for accepted requests.

use crate::{CapabilityId, EventSequence, RequestId};
use taskmanager_core::FailureKind;
use taskmanager_core::ProviderId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RetryDisposition {
    Never,
    RetryNow,
    RetryLater,
    AfterCapabilityChange,
}

/// Failure returned by a blocking native provider before request correlation.
///
/// The native runtime adds request, capability, provider identity and
/// observation time when it creates an [`OperationFailure`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProviderFailure {
    Unsupported,
    RequiresEscalation,
    PermissionDenied,
    MissingDependency,
    TimedOut,
    IdentityChanged,
    TemporarilyUnavailable,
    Rejected,
    ProviderFault,
}

impl ProviderFailure {
    #[must_use]
    pub const fn from_kind(kind: FailureKind) -> Self {
        match kind {
            FailureKind::Unsupported => Self::Unsupported,
            FailureKind::RequiresEscalation => Self::RequiresEscalation,
            FailureKind::PermissionDenied => Self::PermissionDenied,
            FailureKind::MissingDependency => Self::MissingDependency,
            FailureKind::TimedOut => Self::TimedOut,
            FailureKind::IdentityChanged => Self::IdentityChanged,
            FailureKind::TemporarilyUnavailable => Self::TemporarilyUnavailable,
            FailureKind::Rejected => Self::Rejected,
            FailureKind::ProviderFault => Self::ProviderFault,
        }
    }

    #[must_use]
    pub const fn kind(self) -> FailureKind {
        match self {
            Self::Unsupported => FailureKind::Unsupported,
            Self::RequiresEscalation => FailureKind::RequiresEscalation,
            Self::PermissionDenied => FailureKind::PermissionDenied,
            Self::MissingDependency => FailureKind::MissingDependency,
            Self::TimedOut => FailureKind::TimedOut,
            Self::IdentityChanged => FailureKind::IdentityChanged,
            Self::TemporarilyUnavailable => FailureKind::TemporarilyUnavailable,
            Self::Rejected => FailureKind::Rejected,
            Self::ProviderFault => FailureKind::ProviderFault,
        }
    }

    #[must_use]
    pub const fn retry(self) -> RetryDisposition {
        match self {
            Self::Unsupported | Self::IdentityChanged => RetryDisposition::Never,
            Self::RequiresEscalation | Self::PermissionDenied | Self::MissingDependency => {
                RetryDisposition::AfterCapabilityChange
            }
            Self::Rejected => RetryDisposition::RetryNow,
            Self::TimedOut | Self::TemporarilyUnavailable | Self::ProviderFault => {
                RetryDisposition::RetryLater
            }
        }
    }
}

/// Failure of a request that was accepted by the runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationFailure {
    pub request_id: RequestId,
    pub capability: CapabilityId,
    pub sequence: EventSequence,
    pub kind: FailureKind,
    pub retry: RetryDisposition,
    pub provider: Option<ProviderId>,
    pub observed_at_ms: u64,
}

/// A request was not accepted by the bounded runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubmissionErrorKind {
    Busy,
    RuntimeStopped,
    InvalidRequest,
    UnsupportedCapability,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionError {
    pub capability: CapabilityId,
    pub kind: SubmissionErrorKind,
}

/// Delivery-side failure is separate from provider operation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventPortError {
    RuntimeStopped,
}

/// Failure of a system-tray spawn or tray-controller mutation.
///
/// The tray is a process-lifetime object hosted by the frontend rather than a
/// worker-lane capability, so it has its own failure contract: no correlation
/// fields, just a typed reason the frontend can surface or ignore.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrayFailure {
    /// The platform (or frontend shape) cannot host a tray at all.
    Unsupported,
    /// No session bus or no StatusNotifierWatcher is available (Linux).
    MissingDependency,
    /// A transient spawn failure; retrying later may succeed.
    TemporarilyUnavailable,
    /// The mutation was attempted from a thread the tray does not allow
    /// (macOS host); the request was refused without touching the tray.
    WrongThread,
    /// The native adapter refused the request for another reason.
    Rejected,
}
