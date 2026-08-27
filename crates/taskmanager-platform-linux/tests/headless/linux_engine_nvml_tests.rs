use super::*;

#[test]
fn nvml_errors_keep_missing_permission_not_supported_and_transient_distinct() {
    assert_eq!(
        classify_error(&NvmlError::LibraryNotFound),
        NvmlFailureKind::MissingLibrary
    );
    assert_eq!(
        classify_error(&NvmlError::FunctionNotFound),
        NvmlFailureKind::MissingLibrary
    );
    assert_eq!(
        classify_error(&NvmlError::NoPermission),
        NvmlFailureKind::PermissionDenied
    );
    assert_eq!(
        classify_error(&NvmlError::NotSupported),
        NvmlFailureKind::NotSupported
    );
    assert_eq!(
        classify_error(&NvmlError::GpuLost),
        NvmlFailureKind::Transient
    );
    assert_eq!(
        classify_error(&NvmlError::NotFound),
        NvmlFailureKind::Transient
    );
    assert_eq!(
        NvmlFailureKind::Unsupported.device_status(),
        DeviceStatus::Unsupported
    );
}
