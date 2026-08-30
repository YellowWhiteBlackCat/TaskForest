//! Request-correlated CPU package-power acquisition lifecycle.
//!
//! Mirrors [`super::gpu_engine_rows`] minus the device correlation: the read
//! is system-scoped, so there is no target identity to echo or scope `last_good`
//! by — the accepted payload survives replacement and provider failures.

use crate::{RequestAttemptId, RequestCorrelation};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{RaplPowerFailure, RaplPowerSnapshot};
use taskmanager_platform_contract::RequestId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RaplPowerRequestFailure {
    Submission(FailureKind),
    Provider(RaplPowerFailure),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RaplPowerReady {
    pub request_id: RequestId,
    pub snapshot: RaplPowerSnapshot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RaplPowerFailed {
    pub correlation: RequestCorrelation,
    pub failure: RaplPowerRequestFailure,
    pub last_good: Option<RaplPowerReady>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum RaplPowerState {
    #[default]
    Closed,
    Loading {
        correlation: RequestCorrelation,
        last_good: Option<RaplPowerReady>,
    },
    Ready(RaplPowerReady),
    Failed(RaplPowerFailed),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RaplPowerSession {
    state: RaplPowerState,
}

impl RaplPowerSession {
    #[must_use]
    pub const fn state(&self) -> &RaplPowerState {
        &self.state
    }

    pub fn close(&mut self) {
        self.state = RaplPowerState::Closed;
    }

    #[must_use]
    pub fn begin_attempt(&mut self) -> RequestAttemptId {
        let attempt = RequestAttemptId::next();
        let last_good = self.last_good().cloned();
        self.state = RaplPowerState::Loading {
            correlation: RequestCorrelation::Attempt(attempt),
            last_good,
        };
        attempt
    }

    pub fn accept_attempt(&mut self, attempt: RequestAttemptId, request_id: RequestId) -> bool {
        let RaplPowerState::Loading { correlation, .. } = &mut self.state else {
            return false;
        };
        if *correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        *correlation = RequestCorrelation::Request(request_id);
        true
    }

    pub fn reject_attempt(&mut self, attempt: RequestAttemptId, failure: FailureKind) -> bool {
        let RaplPowerState::Loading {
            correlation,
            last_good,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        self.state = RaplPowerState::Failed(RaplPowerFailed {
            correlation: *correlation,
            failure: RaplPowerRequestFailure::Submission(failure),
            last_good: last_good.clone(),
        });
        true
    }

    pub fn complete(&mut self, request_id: RequestId, snapshot: RaplPowerSnapshot) -> bool {
        let RaplPowerState::Loading {
            correlation,
            last_good,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Request(request_id) {
            return false;
        }
        if let Some(failure) = snapshot.failure.clone() {
            self.state = RaplPowerState::Failed(RaplPowerFailed {
                correlation: *correlation,
                failure: RaplPowerRequestFailure::Provider(failure),
                last_good: last_good.clone(),
            });
        } else {
            self.state = RaplPowerState::Ready(RaplPowerReady {
                request_id,
                snapshot,
            });
        }
        true
    }

    pub fn fail(&mut self, request_id: RequestId, failure: FailureKind) -> bool {
        let RaplPowerState::Loading {
            correlation,
            last_good,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Request(request_id) {
            return false;
        }
        self.state = RaplPowerState::Failed(RaplPowerFailed {
            correlation: *correlation,
            failure: RaplPowerRequestFailure::Submission(failure),
            last_good: last_good.clone(),
        });
        true
    }

    #[must_use]
    pub fn retry(&mut self) -> Option<RequestAttemptId> {
        if !matches!(self.state, RaplPowerState::Failed(_)) {
            return None;
        }
        Some(self.begin_attempt())
    }

    fn last_good(&self) -> Option<&RaplPowerReady> {
        match &self.state {
            RaplPowerState::Ready(ready) => Some(ready),
            RaplPowerState::Loading { last_good, .. }
            | RaplPowerState::Failed(RaplPowerFailed { last_good, .. }) => last_good.as_ref(),
            RaplPowerState::Closed => None,
        }
    }
}
