//! Request-correlated command, resource-reveal and URL-open lifecycle.

use crate::{
    CommandLaunchRequest, RequestAttemptId, RequestCorrelation, ResourceRevealRequest, ShellEvent,
    UrlOpenRequest,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_platform_contract::{CapabilityId, RequestId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellUiActionIntent {
    Command(CommandLaunchRequest),
    Reveal(ResourceRevealRequest),
    OpenUrl(UrlOpenRequest),
}

impl ShellUiActionIntent {
    #[must_use]
    pub const fn capability(&self) -> CapabilityId {
        match self {
            Self::Command(_) => CapabilityId::COMMAND_LAUNCH,
            Self::Reveal(_) => CapabilityId::RESOURCE_REVEAL,
            Self::OpenUrl(_) => CapabilityId::URL_OPEN,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellUiActionReceipt {
    CommandLaunched { pid: u32 },
    TargetOpened,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellUiActionReady {
    pub request_id: RequestId,
    pub intent: ShellUiActionIntent,
    pub receipt: ShellUiActionReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellUiActionFailed {
    pub correlation: RequestCorrelation,
    pub intent: ShellUiActionIntent,
    pub failure: FailureKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ShellUiActionState {
    #[default]
    Closed,
    Loading {
        correlation: RequestCorrelation,
        intent: ShellUiActionIntent,
    },
    Ready(ShellUiActionReady),
    Failed(ShellUiActionFailed),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellUiActionSession {
    state: ShellUiActionState,
}

impl ShellUiActionSession {
    #[must_use]
    pub const fn state(&self) -> &ShellUiActionState {
        &self.state
    }

    pub fn close(&mut self) {
        self.state = ShellUiActionState::Closed;
    }

    #[must_use]
    pub fn begin_attempt(&mut self, intent: ShellUiActionIntent) -> RequestAttemptId {
        let attempt = RequestAttemptId::next();
        self.state = ShellUiActionState::Loading {
            correlation: RequestCorrelation::Attempt(attempt),
            intent,
        };
        attempt
    }

    pub fn accept_attempt(&mut self, attempt: RequestAttemptId, request_id: RequestId) -> bool {
        let ShellUiActionState::Loading { correlation, .. } = &mut self.state else {
            return false;
        };
        if *correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        *correlation = RequestCorrelation::Request(request_id);
        true
    }

    pub fn reject_attempt(&mut self, attempt: RequestAttemptId, failure: FailureKind) -> bool {
        let ShellUiActionState::Loading {
            correlation,
            intent,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        self.state = ShellUiActionState::Failed(ShellUiActionFailed {
            correlation: *correlation,
            intent: intent.clone(),
            failure,
        });
        true
    }

    pub fn complete(
        &mut self,
        request_id: RequestId,
        capability: &CapabilityId,
        event: &ShellEvent,
    ) -> bool {
        let ShellUiActionState::Loading {
            correlation,
            intent,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Request(request_id)
            || &intent.capability() != capability
        {
            return false;
        }
        let receipt = match (intent, event) {
            (ShellUiActionIntent::Command(_), ShellEvent::CommandLaunched { pid }) => {
                ShellUiActionReceipt::CommandLaunched { pid: *pid }
            }
            (
                ShellUiActionIntent::Reveal(_) | ShellUiActionIntent::OpenUrl(_),
                ShellEvent::TargetOpened,
            ) => ShellUiActionReceipt::TargetOpened,
            _ => return false,
        };
        self.state = ShellUiActionState::Ready(ShellUiActionReady {
            request_id,
            intent: intent.clone(),
            receipt,
        });
        true
    }

    pub fn fail(
        &mut self,
        request_id: RequestId,
        capability: &CapabilityId,
        failure: FailureKind,
    ) -> bool {
        let ShellUiActionState::Loading {
            correlation,
            intent,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Request(request_id)
            || &intent.capability() != capability
        {
            return false;
        }
        self.state = ShellUiActionState::Failed(ShellUiActionFailed {
            correlation: *correlation,
            intent: intent.clone(),
            failure,
        });
        true
    }

    #[must_use]
    pub fn retry(&mut self) -> Option<RequestAttemptId> {
        let ShellUiActionState::Failed(failed) = &self.state else {
            return None;
        };
        Some(self.begin_attempt(failed.intent.clone()))
    }
}
