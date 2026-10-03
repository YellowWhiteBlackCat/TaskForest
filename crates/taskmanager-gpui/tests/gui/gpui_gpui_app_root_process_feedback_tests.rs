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

#[test]
fn priority_tier_actions_render_localized_feedback() {
    use super::priority_tier_label;
    use taskmanager_core::core::process::PriorityTier;

    for tier in [PriorityTier::High, PriorityTier::Normal, PriorityTier::Low] {
        let feedback =
            process_control_feedback(ProcessControlAction::SetPriority(tier), 42, Ok(()));
        assert!(feedback.contains(priority_tier_label(tier)));
    }
}
