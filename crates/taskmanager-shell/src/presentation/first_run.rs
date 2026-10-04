//! Shared optional-setup status and typed failure copy.

use taskmanager_application::first_run::FirstRunPhase;
use taskmanager_core::core::failure::FailureKind;

#[must_use]
pub const fn first_run_failure_key(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::Unsupported => "first_run.failure_unsupported",
        FailureKind::PermissionDenied | FailureKind::RequiresEscalation => {
            "first_run.failure_permission"
        }
        FailureKind::MissingDependency => "first_run.failure_missing_dependency",
        FailureKind::TimedOut => "first_run.failure_timeout",
        FailureKind::IdentityChanged => "first_run.failure_identity",
        FailureKind::TemporarilyUnavailable => "first_run.failure_unavailable",
        FailureKind::Rejected => "first_run.failure_rejected",
        FailureKind::ProviderFault => "first_run.failure_provider",
    }
}
#[must_use]
pub fn first_run_phase_key(phase: &FirstRunPhase) -> Option<&'static str> {
    match phase {
        FirstRunPhase::Discovering => Some("first_run.discovering"),
        FirstRunPhase::Running => Some("first_run.running"),
        FirstRunPhase::Reverting => Some("first_run.reverting"),
        FirstRunPhase::Restarting => Some("first_run.restarting"),
        FirstRunPhase::RestartRequired => Some("first_run.restart_required"),
        FirstRunPhase::Failed(kind) => Some(first_run_failure_key(*kind)),
        FirstRunPhase::Hidden | FirstRunPhase::Available => None,
    }
}
