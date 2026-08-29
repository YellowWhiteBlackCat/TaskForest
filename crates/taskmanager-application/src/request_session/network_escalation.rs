//! Request-correlated process-network escalation lifecycle.

use crate::{RequestAttemptId, RequestCorrelation};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_platform_contract::RequestId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkEscalationReady {
    pub request_id: RequestId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkEscalationFailed {
    pub correlation: RequestCorrelation,
    pub failure: FailureKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NetworkEscalationState {
    #[default]
    Closed,
    Loading(RequestCorrelation),
    Ready(NetworkEscalationReady),
    Failed(NetworkEscalationFailed),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetworkEscalationSession {
    state: NetworkEscalationState,
}

impl NetworkEscalationSession {
    #[must_use]
    pub const fn state(&self) -> &NetworkEscalationState {
        &self.state
    }

    pub fn close(&mut self) {
        self.state = NetworkEscalationState::Closed;
    }

    #[must_use]
    pub fn begin_attempt(&mut self) -> RequestAttemptId {
        let attempt = RequestAttemptId::next();
        self.state = NetworkEscalationState::Loading(RequestCorrelation::Attempt(attempt));
        attempt
    }

    pub fn accept_attempt(&mut self, attempt: RequestAttemptId, request_id: RequestId) -> bool {
        if self.state != NetworkEscalationState::Loading(RequestCorrelation::Attempt(attempt)) {
            return false;
        }
        self.state = NetworkEscalationState::Loading(RequestCorrelation::Request(request_id));
        true
    }

    pub fn reject_attempt(&mut self, attempt: RequestAttemptId, failure: FailureKind) -> bool {
        let correlation = RequestCorrelation::Attempt(attempt);
        if self.state != NetworkEscalationState::Loading(correlation) {
            return false;
        }
        self.state = NetworkEscalationState::Failed(NetworkEscalationFailed {
            correlation,
            failure,
        });
        true
    }

    pub fn complete(&mut self, request_id: RequestId) -> bool {
        if self.state != NetworkEscalationState::Loading(RequestCorrelation::Request(request_id)) {
            return false;
        }
        self.state = NetworkEscalationState::Ready(NetworkEscalationReady { request_id });
        true
    }

    pub fn fail(&mut self, request_id: RequestId, failure: FailureKind) -> bool {
        let correlation = RequestCorrelation::Request(request_id);
        if self.state != NetworkEscalationState::Loading(correlation) {
            return false;
        }
        self.state = NetworkEscalationState::Failed(NetworkEscalationFailed {
            correlation,
            failure,
        });
        true
    }

    #[must_use]
    pub fn retry(&mut self) -> Option<RequestAttemptId> {
        matches!(self.state, NetworkEscalationState::Failed(_)).then(|| self.begin_attempt())
    }
}
