//! Request-correlated SMBIOS memory acquisition lifecycle.
//!
//! Mirrors [`super::gpu_engine_rows`] minus the device correlation: the read
//! is system-scoped, so there is no target identity to echo or scope `last_good`
//! by — the accepted payload survives replacement and provider failures.

use crate::{RequestAttemptId, RequestCorrelation};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{SmbiosMemoryFailure, SmbiosMemorySnapshot};
use taskmanager_platform_contract::RequestId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmbiosMemoryRequestFailure {
    Submission(FailureKind),
    Provider(SmbiosMemoryFailure),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SmbiosMemoryReady {
    pub request_id: RequestId,
    pub snapshot: SmbiosMemorySnapshot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SmbiosMemoryFailed {
    pub correlation: RequestCorrelation,
    pub failure: SmbiosMemoryRequestFailure,
    pub last_good: Option<SmbiosMemoryReady>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum SmbiosMemoryState {
    #[default]
    Closed,
    Loading {
        correlation: RequestCorrelation,
        last_good: Option<SmbiosMemoryReady>,
    },
    Ready(SmbiosMemoryReady),
    Failed(SmbiosMemoryFailed),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmbiosMemorySession {
    state: SmbiosMemoryState,
}

impl SmbiosMemorySession {
    #[must_use]
    pub const fn state(&self) -> &SmbiosMemoryState {
        &self.state
    }

    pub fn close(&mut self) {
        self.state = SmbiosMemoryState::Closed;
    }

    #[must_use]
    pub fn begin_attempt(&mut self) -> RequestAttemptId {
        let attempt = RequestAttemptId::next();
        let last_good = self.last_good().cloned();
        self.state = SmbiosMemoryState::Loading {
            correlation: RequestCorrelation::Attempt(attempt),
            last_good,
        };
        attempt
    }

    pub fn accept_attempt(&mut self, attempt: RequestAttemptId, request_id: RequestId) -> bool {
        let SmbiosMemoryState::Loading { correlation, .. } = &mut self.state else {
            return false;
        };
        if *correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        *correlation = RequestCorrelation::Request(request_id);
        true
    }

    pub fn reject_attempt(&mut self, attempt: RequestAttemptId, failure: FailureKind) -> bool {
        let SmbiosMemoryState::Loading {
            correlation,
            last_good,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        self.state = SmbiosMemoryState::Failed(SmbiosMemoryFailed {
            correlation: *correlation,
            failure: SmbiosMemoryRequestFailure::Submission(failure),
            last_good: last_good.clone(),
        });
        true
    }

    pub fn complete(&mut self, request_id: RequestId, snapshot: SmbiosMemorySnapshot) -> bool {
        let SmbiosMemoryState::Loading {
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
            self.state = SmbiosMemoryState::Failed(SmbiosMemoryFailed {
                correlation: *correlation,
                failure: SmbiosMemoryRequestFailure::Provider(failure),
                last_good: last_good.clone(),
            });
        } else {
            self.state = SmbiosMemoryState::Ready(SmbiosMemoryReady {
                request_id,
                snapshot,
            });
        }
        true
    }

    pub fn fail(&mut self, request_id: RequestId, failure: FailureKind) -> bool {
        let SmbiosMemoryState::Loading {
            correlation,
            last_good,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Request(request_id) {
            return false;
        }
        self.state = SmbiosMemoryState::Failed(SmbiosMemoryFailed {
            correlation: *correlation,
            failure: SmbiosMemoryRequestFailure::Submission(failure),
            last_good: last_good.clone(),
        });
        true
    }

    #[must_use]
    pub fn retry(&mut self) -> Option<RequestAttemptId> {
        if !matches!(self.state, SmbiosMemoryState::Failed(_)) {
            return None;
        }
        Some(self.begin_attempt())
    }

    fn last_good(&self) -> Option<&SmbiosMemoryReady> {
        match &self.state {
            SmbiosMemoryState::Ready(ready) => Some(ready),
            SmbiosMemoryState::Loading { last_good, .. }
            | SmbiosMemoryState::Failed(SmbiosMemoryFailed { last_good, .. }) => last_good.as_ref(),
            SmbiosMemoryState::Closed => None,
        }
    }
}
