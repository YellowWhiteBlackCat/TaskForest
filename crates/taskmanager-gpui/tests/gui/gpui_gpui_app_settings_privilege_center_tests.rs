use super::*;

#[test]
fn failure_mapping_preserves_the_seven_permission_states() {
    assert_eq!(
        state_from_failure(FailureKind::RequiresEscalation),
        PrivilegeRowState::NeedsAuthorization
    );
    assert_eq!(
        state_from_failure(FailureKind::PermissionDenied),
        PrivilegeRowState::Denied
    );
    assert_eq!(
        state_from_failure(FailureKind::Unsupported),
        PrivilegeRowState::Unsupported
    );
    assert_eq!(
        state_from_failure(FailureKind::ProviderFault),
        PrivilegeRowState::Failed
    );
    assert_eq!(
        state_from_failure(FailureKind::TimedOut),
        PrivilegeRowState::Unavailable
    );
}

#[test]
fn capability_mapping_keeps_unsupported_separate_from_unavailable() {
    assert_eq!(
        capability_state(Some(CapabilityStatus::Unsupported)),
        Some(PrivilegeRowState::Unsupported)
    );
    assert_eq!(
        capability_state(Some(CapabilityStatus::TemporarilyUnavailable)),
        Some(PrivilegeRowState::Unavailable)
    );
    assert_eq!(
        capability_state(Some(CapabilityStatus::Degraded(FailureKind::ProviderFault))),
        Some(PrivilegeRowState::Failed)
    );
}
