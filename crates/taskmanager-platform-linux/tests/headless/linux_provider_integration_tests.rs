use taskmanager_escalation::polkit::SetupScriptFailure;
use taskmanager_platform_contract::ProviderFailure;

use super::{
    NativeSetupScriptProvider, SETUP_HELPER_PATH, SETUP_SCRIPT_PATH, classify_setup_failure,
    classify_shell_error,
};

#[test]
fn permission_denied_is_typed_regardless_of_case() {
    assert_eq!(
        classify_shell_error("xdg-open: permission denied"),
        ProviderFailure::PermissionDenied
    );
    assert_eq!(
        classify_shell_error("Permission Denied while opening /proc/1/root"),
        ProviderFailure::PermissionDenied
    );
}

#[test]
fn missing_targets_classify_as_temporarily_unavailable() {
    for message in [
        "command not supported on this system",
        "No such file or directory (os error 2)",
        "gio: not found",
        "exec: \"xdg-open\": not found",
    ] {
        assert_eq!(
            classify_shell_error(message),
            ProviderFailure::TemporarilyUnavailable,
            "message {message:?}"
        );
    }
}

#[test]
fn anything_else_is_a_plain_rejection() {
    assert_eq!(
        classify_shell_error("kde-open5 exited with status 127"),
        ProviderFailure::Rejected
    );
    assert_eq!(classify_shell_error(""), ProviderFailure::Rejected);
    assert_eq!(
        classify_shell_error("timeout after 5000ms"),
        ProviderFailure::Rejected
    );
}

#[test]
fn setup_descriptor_uses_only_fixed_paths_and_actions() {
    let info = NativeSetupScriptProvider::info();
    assert_eq!(info.path, std::path::Path::new(SETUP_SCRIPT_PATH));
    assert_eq!(
        info.run_command,
        format!("pkexec {SETUP_HELPER_PATH} install")
    );
    assert_eq!(
        info.revert_command,
        format!("pkexec {SETUP_HELPER_PATH} revert")
    );
}

#[test]
fn setup_helper_failures_remain_typed() {
    assert_eq!(
        classify_setup_failure(SetupScriptFailure::PermissionDenied),
        ProviderFailure::PermissionDenied
    );
    assert_eq!(
        classify_setup_failure(SetupScriptFailure::MissingDependency),
        ProviderFailure::MissingDependency
    );
    assert_eq!(
        classify_setup_failure(SetupScriptFailure::Rejected),
        ProviderFailure::Rejected
    );
    assert_eq!(
        classify_setup_failure(SetupScriptFailure::ProviderFault),
        ProviderFailure::ProviderFault
    );
}
