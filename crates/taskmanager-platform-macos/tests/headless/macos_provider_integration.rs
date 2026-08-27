use super::*;

#[test]
fn shell_error_classification_is_stable() {
    assert_eq!(
        classify_shell_error("permission denied"),
        ProviderFailure::PermissionDenied
    );
    assert_eq!(
        classify_shell_error("command not found"),
        ProviderFailure::TemporarilyUnavailable
    );
    assert_eq!(classify_shell_error("boom"), ProviderFailure::Rejected);
}

#[test]
fn pending_notification_and_setup_complete_with_typed_unsupported() {
    // Every SetupScriptAction must resolve to the same honest Unsupported
    // outcome — enumerated from the full action set, never a sample.
    let mut notification = PendingDesktopNotificationProvider;
    assert_eq!(
        notification.notify("t", "b", AlertSeverity::Critical, "target"),
        Err(ProviderFailure::Unsupported)
    );
    let actions = [
        SetupScriptAction::Observe,
        SetupScriptAction::View,
        SetupScriptAction::Run,
        SetupScriptAction::Revert,
        SetupScriptAction::Restart,
    ];
    for action in actions {
        let mut setup = PendingSetupScriptProvider;
        assert_eq!(
            setup.perform(action),
            Err(ProviderFailure::Unsupported),
            "{action:?} must complete with a typed Unsupported outcome"
        );
    }
}
