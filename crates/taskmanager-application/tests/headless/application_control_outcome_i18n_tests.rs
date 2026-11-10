//! One composition for control-outcome copy: no Debug dumps, no source
//! sentences, and the reason follows the active language.

use crate::i18n::{
    Language, control_error_detail, control_outcome_text, service_log_rejection_detail,
    set_language, startup_action_label, t,
};
use taskforest_core::core::failure::FailureKind;

const FAILURES: [FailureKind; 9] = [
    FailureKind::Unsupported,
    FailureKind::PermissionDenied,
    FailureKind::MissingDependency,
    FailureKind::TimedOut,
    FailureKind::IdentityChanged,
    FailureKind::TemporarilyUnavailable,
    FailureKind::Rejected,
    FailureKind::ProviderFault,
    FailureKind::RequiresEscalation,
];

/// `t` reads a process-wide global, so every language switch lives inside
/// ONE sequential test (the same discipline as `t_resolves_and_falls_back`).
#[test]
fn control_outcome_copy_has_one_owner() {
    set_language(Language::En);

    // Every typed failure resolves through the core key vocabulary; the
    // resolver is the only place a key meets a language. No second table.
    for failure in FAILURES {
        assert_eq!(control_error_detail(failure), t(failure.i18n_key()));
        assert!(
            !control_error_detail(failure).contains(&format!("{failure:?}")),
            "the reason must never be a Debug dump: {failure:?}"
        );
        let rejected = service_log_rejection_detail(failure);
        assert!(
            rejected.contains(control_error_detail(failure)),
            "{rejected}"
        );
        assert!(
            !rejected.contains(&format!("{failure:?}")),
            "the service-log rejection must not Debug-dump the failure: {rejected}"
        );
    }

    // Both success templates compose the same action/target slots.
    let accepted = control_outcome_text("Disconnect", "c2", true, Ok(()));
    let completed = control_outcome_text("Disable", "SSH Agent", false, Ok(()));
    assert_eq!(
        accepted,
        t("feedback.session_request_accepted")
            .replace("{action}", "Disconnect")
            .replace("{target}", "c2")
    );
    assert_eq!(
        completed,
        t("feedback.action_succeeded")
            .replace("{action}", "Disable")
            .replace("{target}", "SSH Agent")
    );
    assert_ne!(accepted, completed, "acceptance is not completion");

    // Failure uses the shared reason, never the enum spelling.
    for failure in FAILURES {
        let failed = control_outcome_text("Start", "dbus", false, Err(failure));
        assert_eq!(
            failed,
            t("feedback.action_failed_detail")
                .replace("{action}", "Start")
                .replace("{target}", "dbus")
                .replace("{detail}", control_error_detail(failure))
        );
        assert!(
            !failed.contains(&format!("{failure:?}")),
            "a typed failure must not be Debug-formatted in a notice: {failed}"
        );
    }

    // The startup verb is a typed boolean, resolved by the owner, not a
    // frontend-written "enable"/"disable".
    assert_eq!(startup_action_label(true), t("startup.enable"));
    assert_eq!(startup_action_label(false), t("startup.disable"));

    // The same composition follows the active language — the reason is
    // localized, not an English enum variant.
    set_language(Language::Zh);
    let zh_failure = control_outcome_text("启动", "dbus", false, Err(FailureKind::TimedOut));
    assert!(zh_failure.contains(control_error_detail(FailureKind::TimedOut)));
    assert!(!zh_failure.contains("TimedOut"));
    let zh_rejected = service_log_rejection_detail(FailureKind::Rejected);
    assert!(!zh_rejected.contains("Rejected"));

    set_language(Language::En);
}
