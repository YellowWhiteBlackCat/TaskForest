//! Provider-neutral native GPU field read receipts.

use taskmanager_core::FailureKind;

/// One native scalar/group read before it enters provider merge.
///
/// A value and failure may coexist when one sibling node succeeds while
/// another fails. The scalar assembler converts that state to `Partial`.
#[derive(Debug)]
pub(super) struct GpuFieldRead<T> {
    pub(super) value: Option<T>,
    pub(super) failure: Option<FailureKind>,
}

impl<T> GpuFieldRead<T> {
    pub(super) const fn available(value: T) -> Self {
        Self {
            value: Some(value),
            failure: None,
        }
    }

    pub(super) const fn unavailable(failure: FailureKind) -> Self {
        Self {
            value: None,
            failure: Some(failure),
        }
    }

    pub(super) const fn partial(value: T, failure: FailureKind) -> Self {
        Self {
            value: Some(value),
            failure: Some(failure),
        }
    }
}

pub(super) fn preferred_gpu_failure(
    current: Option<FailureKind>,
    candidate: Option<FailureKind>,
) -> Option<FailureKind> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(
            if gpu_failure_priority(candidate) > gpu_failure_priority(current) {
                candidate
            } else {
                current
            },
        ),
        (current @ Some(_), None) | (None, current @ Some(_)) => current,
        (None, None) => None,
    }
}

const fn gpu_failure_priority(failure: FailureKind) -> u8 {
    match failure {
        // Escalation-aware denial is the most actionable kind (the UI can offer
        // one specific prompt), so it wins a merge against a generic denial or a
        // transient sibling failure.
        FailureKind::RequiresEscalation => 10,
        FailureKind::PermissionDenied => 9,
        FailureKind::IdentityChanged => 8,
        FailureKind::ProviderFault => 7,
        FailureKind::TimedOut => 6,
        FailureKind::TemporarilyUnavailable => 5,
        FailureKind::MissingDependency => 4,
        FailureKind::Rejected => 3,
        FailureKind::Unsupported => 1,
    }
}

pub(super) fn gpu_io_failure(error: &std::io::Error, missing: FailureKind) -> FailureKind {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        std::io::ErrorKind::NotFound => missing,
        std::io::ErrorKind::InvalidData => FailureKind::ProviderFault,
        std::io::ErrorKind::TimedOut => FailureKind::TimedOut,
        _ => FailureKind::TemporarilyUnavailable,
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_hardware_gpu_field_read_tests.rs"]
mod tests;
