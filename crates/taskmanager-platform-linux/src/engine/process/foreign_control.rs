//! Permission-denied fallback for foreign-process control.
//!
//! Direct same-user operations stay in the Linux provider. Only a typed
//! `PermissionDenied` after the normal identity checks may cross the
//! feature-specific pkexec helper; identity races and unsupported operations
//! never trigger escalation.

use taskmanager_core::{FailureKind, FrozenProcessIdentity, ProcessBatchAction, ProcessSignal};
use taskmanager_escalation::polkit::{
    ForeignProcessControlFailure, ForeignProcessControlOperation, ForeignProcessControlOutcome,
    ForeignProcessControlTarget, invoke_foreign_process_control,
};

pub(crate) fn batch_operation(action: ProcessBatchAction) -> ForeignProcessControlOperation {
    match action {
        ProcessBatchAction::End => ForeignProcessControlOperation::End,
        ProcessBatchAction::Kill => ForeignProcessControlOperation::Kill,
        ProcessBatchAction::Suspend => ForeignProcessControlOperation::Suspend,
        ProcessBatchAction::Resume => ForeignProcessControlOperation::Resume,
        ProcessBatchAction::SetPriority(tier) => {
            ForeignProcessControlOperation::SetPriority(tier.canonical_nice())
        }
    }
}

pub(crate) fn signal_operation(signal: ProcessSignal) -> ForeignProcessControlOperation {
    use taskmanager_escalation::polkit::ForeignProcessSignal;
    let signal = match signal {
        ProcessSignal::Terminate => ForeignProcessSignal::Terminate,
        ProcessSignal::Kill => ForeignProcessSignal::Kill,
        ProcessSignal::Stop => ForeignProcessSignal::Stop,
        ProcessSignal::Continue => ForeignProcessSignal::Continue,
        ProcessSignal::Hangup => ForeignProcessSignal::Hangup,
        ProcessSignal::Interrupt => ForeignProcessSignal::Interrupt,
        ProcessSignal::User1 => ForeignProcessSignal::User1,
        ProcessSignal::User2 => ForeignProcessSignal::User2,
    };
    ForeignProcessControlOperation::Signal(signal)
}

pub(crate) fn affinity_operation(cpus: &[u32]) -> ForeignProcessControlOperation {
    ForeignProcessControlOperation::SetAffinity(cpus.to_vec())
}

/// Finish a direct control attempt, escalating only the specific permission
/// failure. The helper's typed result is mapped back into the shared failure
/// vocabulary; helper text never becomes UI authority.
pub(crate) fn finish_with_escalation(
    target: &FrozenProcessIdentity,
    operation: ForeignProcessControlOperation,
    direct: Result<(), FailureKind>,
) -> Result<(), FailureKind> {
    let Err(FailureKind::PermissionDenied) = direct else {
        return direct;
    };
    let Some(helper_target) = target
        .authoritative_start_token()
        .and_then(|token| ForeignProcessControlTarget::new(target.pid, token))
    else {
        return Err(FailureKind::IdentityChanged);
    };
    match invoke_foreign_process_control(helper_target, operation) {
        ForeignProcessControlOutcome::Applied => Ok(()),
        ForeignProcessControlOutcome::Failed { kind, .. } => Err(map_failure(kind)),
        ForeignProcessControlOutcome::Unavailable { reason, .. } => Err(match reason {
            taskmanager_escalation::EscalationDenialReason::Unsupported => FailureKind::Unsupported,
            taskmanager_escalation::EscalationDenialReason::PermissionDenied => {
                FailureKind::PermissionDenied
            }
            taskmanager_escalation::EscalationDenialReason::AuthorizationUnavailable => {
                FailureKind::TemporarilyUnavailable
            }
            // Keep helper absence distinct in the shared data contract: the
            // user may install/enable the one-feature helper later.
            taskmanager_escalation::EscalationDenialReason::HelperUnavailable => {
                FailureKind::RequiresEscalation
            }
            taskmanager_escalation::EscalationDenialReason::HelperProtocolViolation => {
                FailureKind::ProviderFault
            }
        }),
    }
}

fn map_failure(failure: ForeignProcessControlFailure) -> FailureKind {
    match failure {
        ForeignProcessControlFailure::IdentityChanged => FailureKind::IdentityChanged,
        ForeignProcessControlFailure::PermissionDenied => FailureKind::PermissionDenied,
        ForeignProcessControlFailure::Unsupported => FailureKind::Unsupported,
        ForeignProcessControlFailure::Rejected => FailureKind::Rejected,
        ForeignProcessControlFailure::OperationFailed => FailureKind::ProviderFault,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_process_foreign_control_tests.rs"]
mod tests;
