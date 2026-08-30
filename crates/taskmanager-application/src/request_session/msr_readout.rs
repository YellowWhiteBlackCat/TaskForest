//! Request-correlated CPU MSR-readout acquisition lifecycle.
//!
//! Mirrors [`super::rapl_power`]: the read is system-scoped, so there is no
//! target identity to echo or scope `last_good` by — the accepted payload
//! survives replacement and provider failures.

use crate::{RequestAttemptId, RequestCorrelation};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{MsrReadoutFailure, MsrReadoutSnapshot};
use taskmanager_platform_contract::RequestId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MsrReadoutRequestFailure {
    Submission(FailureKind),
    Provider(MsrReadoutFailure),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MsrReadoutReady {
    pub request_id: RequestId,
    pub snapshot: MsrReadoutSnapshot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MsrReadoutFailed {
    pub correlation: RequestCorrelation,
    pub failure: MsrReadoutRequestFailure,
    pub last_good: Option<MsrReadoutReady>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum MsrReadoutState {
    #[default]
    Closed,
    Loading {
        correlation: RequestCorrelation,
        last_good: Option<MsrReadoutReady>,
    },
    Ready(MsrReadoutReady),
    Failed(MsrReadoutFailed),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MsrReadoutSession {
    state: MsrReadoutState,
}

impl MsrReadoutSession {
    #[must_use]
    pub const fn state(&self) -> &MsrReadoutState {
        &self.state
    }

    pub fn close(&mut self) {
        self.state = MsrReadoutState::Closed;
    }

    #[must_use]
    pub fn begin_attempt(&mut self) -> RequestAttemptId {
        let attempt = RequestAttemptId::next();
        let last_good = self.last_good().cloned();
        self.state = MsrReadoutState::Loading {
            correlation: RequestCorrelation::Attempt(attempt),
            last_good,
        };
        attempt
    }

    pub fn accept_attempt(&mut self, attempt: RequestAttemptId, request_id: RequestId) -> bool {
        let MsrReadoutState::Loading { correlation, .. } = &mut self.state else {
            return false;
        };
        if *correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        *correlation = RequestCorrelation::Request(request_id);
        true
    }

    pub fn reject_attempt(&mut self, attempt: RequestAttemptId, failure: FailureKind) -> bool {
        let MsrReadoutState::Loading {
            correlation,
            last_good,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        self.state = MsrReadoutState::Failed(MsrReadoutFailed {
            correlation: *correlation,
            failure: MsrReadoutRequestFailure::Submission(failure),
            last_good: last_good.clone(),
        });
        true
    }

    pub fn complete(&mut self, request_id: RequestId, snapshot: MsrReadoutSnapshot) -> bool {
        let MsrReadoutState::Loading {
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
            self.state = MsrReadoutState::Failed(MsrReadoutFailed {
                correlation: *correlation,
                failure: MsrReadoutRequestFailure::Provider(failure),
                last_good: last_good.clone(),
            });
        } else {
            self.state = MsrReadoutState::Ready(MsrReadoutReady {
                request_id,
                snapshot,
            });
        }
        true
    }

    pub fn fail(&mut self, request_id: RequestId, failure: FailureKind) -> bool {
        let MsrReadoutState::Loading {
            correlation,
            last_good,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Request(request_id) {
            return false;
        }
        self.state = MsrReadoutState::Failed(MsrReadoutFailed {
            correlation: *correlation,
            failure: MsrReadoutRequestFailure::Submission(failure),
            last_good: last_good.clone(),
        });
        true
    }

    #[must_use]
    pub fn retry(&mut self) -> Option<RequestAttemptId> {
        if !matches!(self.state, MsrReadoutState::Failed(_)) {
            return None;
        }
        Some(self.begin_attempt())
    }

    fn last_good(&self) -> Option<&MsrReadoutReady> {
        match &self.state {
            MsrReadoutState::Ready(ready) => Some(ready),
            MsrReadoutState::Loading { last_good, .. }
            | MsrReadoutState::Failed(MsrReadoutFailed { last_good, .. }) => last_good.as_ref(),
            MsrReadoutState::Closed => None,
        }
    }
}
