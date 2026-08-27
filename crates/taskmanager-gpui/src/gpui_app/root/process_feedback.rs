//! Typed process-control user feedback.

use crate::i18n;
use taskmanager_application::{FailureKind, PriorityTier, ProcessSignal};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessControlAction {
    EndTask,
    EndProcessTree,
    Kill,
    Suspend,
    Resume,
    Signal(ProcessSignal),
    SetPriority(PriorityTier),
    SetAffinity,
}

pub(super) fn priority_tier_label(tier: PriorityTier) -> &'static str {
    taskmanager_shell::presentation::priority_tier_label(tier)
}

fn process_control_action_label(action: ProcessControlAction) -> String {
    match action {
        ProcessControlAction::EndTask => i18n::t("proc.end_task").to_string(),
        ProcessControlAction::EndProcessTree => i18n::t("proc.end_process_tree").to_string(),
        ProcessControlAction::Kill => i18n::t("proc.kill").to_string(),
        ProcessControlAction::Suspend => i18n::t("proc.suspend").to_string(),
        ProcessControlAction::Resume => i18n::t("proc.resume").to_string(),
        ProcessControlAction::Signal(signal) => {
            format!("{} {signal:?}", i18n::t("common.signal"))
        }
        ProcessControlAction::SetPriority(tier) => {
            format!(
                "{} ({})",
                i18n::t("proc.priority"),
                priority_tier_label(tier)
            )
        }
        ProcessControlAction::SetAffinity => i18n::t("proc.affinity").to_string(),
    }
}

pub(crate) fn process_control_feedback(
    action: ProcessControlAction,
    pid: u32,
    result: Result<(), FailureKind>,
) -> String {
    let action = process_control_action_label(action);
    match result {
        Ok(()) => format!(
            "\u{2713} {}",
            i18n::t("feedback.process_action_succeeded")
                .replace("{action}", &action)
                .replace("{pid}", &pid.to_string())
        ),
        Err(kind) => format!(
            "\u{26a0} {}",
            i18n::t("feedback.process_action_failed")
                .replace("{action}", &action)
                .replace("{pid}", &pid.to_string())
                .replace("{reason}", &typed_failure_reason(kind))
        ),
    }
}

fn typed_failure_reason(kind: FailureKind) -> String {
    match kind {
        FailureKind::PermissionDenied | FailureKind::RequiresEscalation => {
            i18n::t("feedback.permission_denied").to_string()
        }
        FailureKind::IdentityChanged => i18n::t("feedback.process_gone").to_string(),
        FailureKind::Unsupported => i18n::t("feedback.unsupported").to_string(),
        FailureKind::MissingDependency | FailureKind::TemporarilyUnavailable => {
            i18n::t("feedback.system_error").replace("{detail}", "process provider unavailable")
        }
        FailureKind::TimedOut => {
            i18n::t("feedback.system_error").replace("{detail}", "process control timed out")
        }
        FailureKind::Rejected => {
            i18n::t("feedback.system_error").replace("{detail}", "process control rejected")
        }
        FailureKind::ProviderFault => {
            i18n::t("feedback.system_error").replace("{detail}", "process provider failed")
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_process_feedback_tests.rs"]
mod tests;
