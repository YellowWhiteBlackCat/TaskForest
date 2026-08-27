use super::*;

#[test]
fn session_control_classification_preserves_typed_failures() {
    assert_eq!(
        classify_session_control_error("No session '7' known"),
        ProviderFailure::IdentityChanged
    );
    assert_eq!(
        classify_session_control_error("Permission denied"),
        ProviderFailure::PermissionDenied
    );
    assert_eq!(
        classify_session_control_error("No such file or directory"),
        ProviderFailure::TemporarilyUnavailable
    );
}
