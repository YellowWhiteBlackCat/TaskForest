//! Typed failure ordering for the desktop application catalog.

use taskmanager_core::{FailureKind, ProcessMetadataFailure};

pub(super) fn classify_io(error: &std::io::Error) -> ProcessMetadataFailure {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => ProcessMetadataFailure::PermissionDenied,
        std::io::ErrorKind::NotFound => ProcessMetadataFailure::NotFound,
        _ => ProcessMetadataFailure::ProviderFault,
    }
}

pub(super) fn record_failure(
    slot: &mut Option<ProcessMetadataFailure>,
    failure: ProcessMetadataFailure,
) {
    if slot.is_none_or(|current| failure_priority(failure) > failure_priority(current)) {
        *slot = Some(failure);
    }
}

pub(super) fn stronger_metadata_failure(
    left: ProcessMetadataFailure,
    right: ProcessMetadataFailure,
) -> ProcessMetadataFailure {
    if failure_priority(right) > failure_priority(left) {
        right
    } else {
        left
    }
}

const fn failure_priority(failure: ProcessMetadataFailure) -> u8 {
    match failure {
        // Missing optional catalog data is weaker than provider failures.
        ProcessMetadataFailure::Unsupported => 2,
        ProcessMetadataFailure::PermissionDenied => 8,
        ProcessMetadataFailure::NotFound => 3,
        ProcessMetadataFailure::PidRace => 9,
        ProcessMetadataFailure::ProviderFault => 5,
    }
}

pub(super) const fn shared_failure(failure: ProcessMetadataFailure) -> FailureKind {
    match failure {
        ProcessMetadataFailure::Unsupported => FailureKind::Unsupported,
        ProcessMetadataFailure::PermissionDenied => FailureKind::PermissionDenied,
        ProcessMetadataFailure::NotFound => FailureKind::MissingDependency,
        ProcessMetadataFailure::PidRace => FailureKind::IdentityChanged,
        ProcessMetadataFailure::ProviderFault => FailureKind::ProviderFault,
    }
}
