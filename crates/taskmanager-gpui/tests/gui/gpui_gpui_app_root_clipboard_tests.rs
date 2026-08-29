use taskmanager_core::core::failure::FailureKind;

use super::process_failure_message;

#[test]
fn every_failure_kind_has_a_display_message() {
    for kind in [
        FailureKind::PermissionDenied,
        FailureKind::RequiresEscalation,
        FailureKind::IdentityChanged,
        FailureKind::Unsupported,
        FailureKind::MissingDependency,
        FailureKind::TimedOut,
        FailureKind::TemporarilyUnavailable,
        FailureKind::Rejected,
        FailureKind::ProviderFault,
    ] {
        let message = process_failure_message(kind);
        assert!(
            !message.is_empty(),
            "failure kind {kind:?} must not produce an empty message"
        );
    }
}

#[test]
fn denial_and_escalation_share_the_denial_text() {
    assert_eq!(
        process_failure_message(FailureKind::PermissionDenied),
        "permission denied"
    );
    assert_eq!(
        process_failure_message(FailureKind::RequiresEscalation),
        "permission denied",
        "escalatable denial folds into the denial message"
    );
}

#[test]
fn identity_and_timeout_keep_their_specific_meanings() {
    assert_eq!(
        process_failure_message(FailureKind::IdentityChanged),
        "process does not exist or identity changed"
    );
    assert_eq!(
        process_failure_message(FailureKind::TimedOut),
        "process control timed out"
    );
}
