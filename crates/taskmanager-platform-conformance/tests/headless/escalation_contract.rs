use taskmanager_platform_contract::{CapabilityStatus, ProviderFailure};

use super::{admits_escalation, assert_capability_failure_status, projected_capability_status};

#[test]
fn escalatable_denial_publishes_requires_escalation_not_permission_required() {
    let failure = ProviderFailure::RequiresEscalation;
    assert!(admits_escalation(failure));
    assert_eq!(
        assert_capability_failure_status(failure, CapabilityStatus::RequiresEscalation),
        Ok(())
    );
    assert!(
        assert_capability_failure_status(failure, CapabilityStatus::PermissionRequired).is_err(),
        "an escalatable denial must not be folded onto the plain permission gate",
    );
}

#[test]
fn hard_denial_publishes_permission_required_not_requires_escalation() {
    let failure = ProviderFailure::PermissionDenied;
    assert!(!admits_escalation(failure));
    assert_eq!(
        assert_capability_failure_status(failure, CapabilityStatus::PermissionRequired),
        Ok(())
    );
    assert!(
        assert_capability_failure_status(failure, CapabilityStatus::RequiresEscalation).is_err(),
        "a hard denial must not fabricate an escalation offer",
    );
}

#[test]
fn transient_failure_publishes_temporarily_unavailable() {
    for failure in [
        ProviderFailure::TimedOut,
        ProviderFailure::TemporarilyUnavailable,
        ProviderFailure::Rejected,
    ] {
        assert!(!admits_escalation(failure));
        assert_eq!(
            assert_capability_failure_status(failure, CapabilityStatus::TemporarilyUnavailable),
            Ok(())
        );
        assert!(
            assert_capability_failure_status(failure, CapabilityStatus::PermissionRequired)
                .is_err(),
            "{failure:?} has no permission or escalation meaning",
        );
    }
}

#[test]
fn contract_projection_covers_every_provider_failure() {
    for (failure, expected) in [
        (ProviderFailure::Unsupported, CapabilityStatus::Unsupported),
        (
            ProviderFailure::RequiresEscalation,
            CapabilityStatus::RequiresEscalation,
        ),
        (
            ProviderFailure::PermissionDenied,
            CapabilityStatus::PermissionRequired,
        ),
        (
            ProviderFailure::MissingDependency,
            CapabilityStatus::MissingDependency,
        ),
        (
            ProviderFailure::TimedOut,
            CapabilityStatus::TemporarilyUnavailable,
        ),
        (ProviderFailure::IdentityChanged, CapabilityStatus::Stale),
        (
            ProviderFailure::TemporarilyUnavailable,
            CapabilityStatus::TemporarilyUnavailable,
        ),
        (
            ProviderFailure::Rejected,
            CapabilityStatus::TemporarilyUnavailable,
        ),
        (ProviderFailure::ProviderFault, CapabilityStatus::Stale),
    ] {
        assert_eq!(projected_capability_status(failure), expected);
        assert_eq!(assert_capability_failure_status(failure, expected), Ok(()));
    }
}
