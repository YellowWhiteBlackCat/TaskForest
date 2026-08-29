use super::{diagnostic_failure_feedback_key, diagnostic_failure_message};
use taskmanager_core::core::diagnostics::{DiagnosticBundleError, DiagnosticBundleErrorKind};

#[test]
fn every_typed_failure_maps_to_localized_feedback_without_raw_detail() {
    let cases = [
        (
            DiagnosticBundleErrorKind::InvalidSource,
            "diagnostics.failure_invalid_source",
        ),
        (
            DiagnosticBundleErrorKind::Encode,
            "diagnostics.failure_encode",
        ),
        (DiagnosticBundleErrorKind::Io, "diagnostics.failure_io"),
        (DiagnosticBundleErrorKind::Busy, "diagnostics.failure_busy"),
        (
            DiagnosticBundleErrorKind::Unavailable,
            "diagnostics.failure_unavailable",
        ),
    ];

    for (kind, key) in cases {
        let error = DiagnosticBundleError::with_detail(kind, "/home/<user>/private");
        assert_eq!(diagnostic_failure_feedback_key(kind), key);
        let message = diagnostic_failure_message(&error);
        assert!(message.contains(taskmanager_application::i18n::t(key)));
        assert!(!message.contains("alice"));
    }
}
