use super::*;

#[test]
fn every_typed_control_failure_has_actionable_copy() {
    for kind in [
        FailureKind::Unsupported,
        FailureKind::TemporarilyUnavailable,
        FailureKind::PermissionDenied,
        FailureKind::MissingDependency,
        FailureKind::TimedOut,
        FailureKind::Rejected,
        FailureKind::IdentityChanged,
        FailureKind::ProviderFault,
    ] {
        assert!(!control_error_detail(kind).trim().is_empty());
    }
}

#[test]
fn session_actions_keep_distinct_labels() {
    assert_ne!(
        session_action_label(SessionControlAction::Disconnect),
        session_action_label(SessionControlAction::Lock)
    );
    assert!(session_target("7").ends_with('7'));
    let _ = crate::gpui_app::users_view::ActionFeedback::from_result(
        &Ok(()),
        session_action_label(SessionControlAction::Lock),
        &session_target("7"),
    );
}
