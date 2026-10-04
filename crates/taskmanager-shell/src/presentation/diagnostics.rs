//! Shared sanitized preview text and privacy-safe error presentation.

use taskmanager_application::i18n::t;
use taskmanager_core::core::diagnostics::{
    DiagnosticBundleError, DiagnosticBundleErrorKind, DiagnosticPreview,
};

#[must_use]
pub const fn diagnostic_failure_feedback_key(kind: DiagnosticBundleErrorKind) -> &'static str {
    match kind {
        DiagnosticBundleErrorKind::InvalidSource => "diagnostics.failure_invalid_source",
        DiagnosticBundleErrorKind::InvalidTarget => "diagnostics.failure_invalid_target",
        DiagnosticBundleErrorKind::Encode => "diagnostics.failure_encode",
        DiagnosticBundleErrorKind::Io => "diagnostics.failure_io",
        DiagnosticBundleErrorKind::Busy => "diagnostics.failure_busy",
        DiagnosticBundleErrorKind::Unavailable => "diagnostics.failure_unavailable",
    }
}

#[must_use]
pub fn diagnostic_failure_message(error: &DiagnosticBundleError) -> String {
    t("diagnostics.failed_detail")
        .replace("{reason}", t(diagnostic_failure_feedback_key(error.kind())))
}

#[must_use]
pub fn diagnostic_redaction_summary(preview: &DiagnosticPreview) -> String {
    t("diagnostics.redaction_summary")
        .replace("{total}", &preview.redactions.total().to_string())
        .replace("{users}", &preview.redactions.usernames.to_string())
        .replace("{paths}", &preview.redactions.paths.to_string())
        .replace(
            "{ips}",
            &(preview.redactions.ipv4_addresses + preview.redactions.ipv6_addresses).to_string(),
        )
}

#[must_use]
pub fn diagnostic_preview_text(preview: &DiagnosticPreview) -> String {
    let mut text = diagnostic_redaction_summary(preview);
    for file in &preview.files {
        text.push_str(&format!(
            "\n\n{} · {} B\n{}\n{}",
            file.name, file.bytes, file.sha256, file.excerpt
        ));
    }
    text
}

#[cfg(test)]
#[path = "../../tests/headless/presentation_diagnostics.rs"]
mod tests;
