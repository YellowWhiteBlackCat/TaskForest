use super::*;

#[test]
fn failure_kinds_have_stable_snake_case_tokens() {
    let cases = [
        (DiagnosticBundleErrorKind::InvalidSource, "invalid_source"),
        (DiagnosticBundleErrorKind::InvalidTarget, "invalid_target"),
        (DiagnosticBundleErrorKind::Encode, "encode"),
        (DiagnosticBundleErrorKind::Io, "io"),
        (DiagnosticBundleErrorKind::Busy, "busy"),
        (DiagnosticBundleErrorKind::Unavailable, "unavailable"),
    ];

    for (kind, token) in cases {
        assert_eq!(kind.stable_code(), token);
        assert_eq!(
            serde_json::to_string(&kind).expect("error kind must serialize"),
            format!(r#""{token}""#)
        );
    }
}

#[test]
fn display_is_stable_while_detail_remains_opt_in() {
    let error = DiagnosticBundleError::with_detail(
        DiagnosticBundleErrorKind::Io,
        "/home/<user>/private diagnostics path",
    );

    assert_eq!(error.to_string(), "io");
    assert_eq!(
        error.detail(),
        Some("/home/<user>/private diagnostics path")
    );
    assert!(!error.to_string().contains("alice"));
}
