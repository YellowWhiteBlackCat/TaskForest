use super::*;

#[test]
fn native_io_classifier_preserves_actionable_failure_kinds() {
    assert_eq!(
        gpu_io_failure(
            &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            FailureKind::Unsupported,
        ),
        FailureKind::PermissionDenied
    );
    assert_eq!(
        gpu_io_failure(
            &std::io::Error::from(std::io::ErrorKind::NotFound),
            FailureKind::IdentityChanged,
        ),
        FailureKind::IdentityChanged
    );
    assert_eq!(
        gpu_io_failure(
            &std::io::Error::from(std::io::ErrorKind::InvalidData),
            FailureKind::Unsupported,
        ),
        FailureKind::ProviderFault
    );
}
