use super::{ProcessControlAction, process_control_feedback};
use taskmanager_application::i18n::t;
use taskmanager_core::core::failure::FailureKind;

#[test]
fn typed_failures_render_their_semantic_reason() {
    for (kind, expected) in [
        (
            FailureKind::PermissionDenied,
            t("feedback.permission_denied"),
        ),
        (FailureKind::IdentityChanged, t("feedback.process_gone")),
        (FailureKind::Unsupported, t("feedback.unsupported")),
        (
            FailureKind::MissingDependency,
            "process provider unavailable",
        ),
        (FailureKind::TimedOut, "process control timed out"),
        (FailureKind::Rejected, "process control rejected"),
        (FailureKind::ProviderFault, "process provider failed"),
    ] {
        let feedback = process_control_feedback(ProcessControlAction::EndTask, 42, Err(kind));
        assert!(
            feedback.contains(expected),
            "{kind:?} feedback lost its semantic reason: {feedback}"
        );
    }
}
