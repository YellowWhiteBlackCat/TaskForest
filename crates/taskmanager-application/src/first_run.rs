//! Optional setup discovery and correlated, explicit native actions (ADR-040).

use crate::{CorrelatedSetupScriptEvent, PlatformClient, PlatformEventBatch, SetupScriptRequest};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::setup::{SetupScriptAction, SetupScriptEvent, SetupScriptInfo};
use taskmanager_platform_contract::{
    CapabilityId, OperationFailure, RequestId, SubmissionErrorKind,
};

pub const DOCUMENTATION_URL: &str = "https://github.com/YellowWhiteBlackCat/TaskForest";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FirstRunPhase {
    #[default]
    Hidden,
    Discovering,
    Available,
    Running,
    Reverting,
    RestartRequired,
    Restarting,
    Failed(FailureKind),
}

/// Immutable renderer projection. Only the controller mutates the current view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirstRunUiState {
    pub phase: FirstRunPhase,
    pub info: Option<SetupScriptInfo>,
    pub last_action: Option<SetupScriptAction>,
    pending: Option<(RequestId, SetupScriptAction)>,
    observation_failure: Option<FailureKind>,
}

impl FirstRunUiState {
    #[must_use]
    pub fn action_pending(&self) -> bool {
        self.pending
            .is_some_and(|(_, action)| action != SetupScriptAction::Observe)
    }

    #[must_use]
    pub const fn observation_failure(&self) -> Option<FailureKind> {
        self.observation_failure
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FirstRunCompletion {
    #[default]
    Unchanged,
    Updated,
    Restart,
}

/// Owns descriptor, phases and the sole in-flight native request. Completion
/// changes facts; the frontend's primary surface alone owns visibility.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirstRunController {
    view: FirstRunUiState,
}

impl FirstRunController {
    #[must_use]
    pub fn from_observation(info: Option<SetupScriptInfo>) -> Self {
        Self {
            view: FirstRunUiState {
                phase: if info.is_some() {
                    FirstRunPhase::Available
                } else {
                    FirstRunPhase::Hidden
                },
                info,
                ..Default::default()
            },
        }
    }

    #[must_use]
    pub const fn view(&self) -> &FirstRunUiState {
        &self.view
    }

    /// Quiet background observation never requests a surface transition.
    pub fn observe(&mut self, platform: Option<&mut PlatformClient>, now_ms: u64) -> bool {
        if self.view.pending.is_some() {
            return false;
        }
        self.view.phase = FirstRunPhase::Discovering;
        self.view.observation_failure = None;
        match submit(platform, SetupScriptAction::Observe, now_ms) {
            Ok(id) => {
                self.view.pending = Some((id, SetupScriptAction::Observe));
                true
            }
            Err(kind) => {
                self.reject(SetupScriptAction::Observe, kind);
                false
            }
        }
    }

    /// A repeated activation cannot submit while another action is pending.
    /// Command strings are descriptive metadata and never enter this request.
    pub fn request(
        &mut self,
        action: SetupScriptAction,
        platform: Option<&mut PlatformClient>,
        now_ms: u64,
    ) -> bool {
        if self.view.pending.is_some() {
            return false;
        }
        let rejected = if action == SetupScriptAction::Observe
            || (self.view.info.is_none() && action != SetupScriptAction::Restart)
        {
            Some(FailureKind::Unsupported)
        } else if action == SetupScriptAction::Restart
            && self.view.phase != FirstRunPhase::RestartRequired
        {
            Some(FailureKind::Rejected)
        } else {
            None
        };
        self.view.last_action = Some(action);
        if let Some(kind) = rejected {
            self.view.phase = FirstRunPhase::Failed(kind);
            return false;
        }
        match submit(platform, action, now_ms) {
            Ok(id) => {
                self.view.pending = Some((id, action));
                self.view.phase = match action {
                    SetupScriptAction::Run => FirstRunPhase::Running,
                    SetupScriptAction::Revert => FirstRunPhase::Reverting,
                    SetupScriptAction::Restart => FirstRunPhase::Restarting,
                    SetupScriptAction::View
                        if self.view.phase == FirstRunPhase::RestartRequired =>
                    {
                        FirstRunPhase::RestartRequired
                    }
                    SetupScriptAction::View => FirstRunPhase::Available,
                    SetupScriptAction::Observe => FirstRunPhase::Discovering,
                };
                true
            }
            Err(kind) => {
                self.reject(action, kind);
                false
            }
        }
    }

    pub fn complete(&mut self, event: &CorrelatedSetupScriptEvent) -> FirstRunCompletion {
        let Some((id, requested)) = self.view.pending else {
            return FirstRunCompletion::Unchanged;
        };
        if id != event.request_id || event.capability != CapabilityId::FIRST_RUN_SETUP {
            return FirstRunCompletion::Unchanged;
        }
        self.view.pending = None;
        match (requested, &event.event) {
            (SetupScriptAction::Observe, SetupScriptEvent::Observed(info)) => {
                self.view.info = info.clone();
                self.view.phase = if info.is_some() {
                    FirstRunPhase::Available
                } else {
                    FirstRunPhase::Hidden
                };
                self.view.observation_failure = None;
            }
            (action, SetupScriptEvent::ActionCompleted { action: completed })
                if action == *completed && action != SetupScriptAction::Observe =>
            {
                self.view.phase = match action {
                    SetupScriptAction::Run => FirstRunPhase::RestartRequired,
                    SetupScriptAction::Restart => FirstRunPhase::Restarting,
                    SetupScriptAction::View
                        if self.view.phase == FirstRunPhase::RestartRequired =>
                    {
                        FirstRunPhase::RestartRequired
                    }
                    SetupScriptAction::Revert
                    | SetupScriptAction::View
                    | SetupScriptAction::Observe => FirstRunPhase::Available,
                };
                if action == SetupScriptAction::Restart {
                    return FirstRunCompletion::Restart;
                }
            }
            _ => self.reject(requested, FailureKind::ProviderFault),
        }
        FirstRunCompletion::Updated
    }

    pub fn fail(&mut self, failure: &OperationFailure) -> FirstRunCompletion {
        let Some((id, action)) = self.view.pending else {
            return FirstRunCompletion::Unchanged;
        };
        if failure.capability != CapabilityId::FIRST_RUN_SETUP || failure.request_id != id {
            return FirstRunCompletion::Unchanged;
        }
        self.view.pending = None;
        self.reject(action, failure.kind);
        FirstRunCompletion::Updated
    }

    pub fn fold_batch(&mut self, batch: &PlatformEventBatch) -> FirstRunCompletion {
        let mut outcome = FirstRunCompletion::Unchanged;
        for event in &batch.setup_script_events {
            let change = self.complete(event);
            match change {
                FirstRunCompletion::Restart => outcome = FirstRunCompletion::Restart,
                FirstRunCompletion::Updated if outcome != FirstRunCompletion::Restart => {
                    outcome = FirstRunCompletion::Updated
                }
                FirstRunCompletion::Unchanged | FirstRunCompletion::Updated => {}
            }
        }
        for failure in &batch.failures {
            if self.fail(failure) == FirstRunCompletion::Updated
                && outcome != FirstRunCompletion::Restart
            {
                outcome = FirstRunCompletion::Updated;
            }
        }
        outcome
    }

    fn reject(&mut self, action: SetupScriptAction, kind: FailureKind) {
        if action == SetupScriptAction::Observe {
            self.view.phase = FirstRunPhase::Hidden;
            self.view.info = None;
            self.view.observation_failure = Some(kind);
        } else {
            self.view.phase = FirstRunPhase::Failed(kind);
        }
    }
}

fn submit(
    platform: Option<&mut PlatformClient>,
    action: SetupScriptAction,
    now_ms: u64,
) -> Result<RequestId, FailureKind> {
    let Some(platform) = platform else {
        return Err(FailureKind::TemporarilyUnavailable);
    };
    platform
        .submit_setup_script(SetupScriptRequest { action }, now_ms)
        .map_err(|error| match error.kind {
            SubmissionErrorKind::UnsupportedCapability => FailureKind::Unsupported,
            SubmissionErrorKind::Busy | SubmissionErrorKind::RuntimeStopped => {
                FailureKind::TemporarilyUnavailable
            }
            SubmissionErrorKind::InvalidRequest => FailureKind::Rejected,
        })
}

#[cfg(test)]
#[path = "../tests/headless/first_run_tests.rs"]
mod tests;
