//! Request-correlated per-engine GPU acquisition lifecycle.

use crate::{
    DeviceId, FailureKind, GpuEngineRowsFailure, GpuEngineRowsSnapshot, RequestAttemptId,
    RequestCorrelation, RequestId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GpuEngineRowsRequestFailure {
    Submission(FailureKind),
    Provider(GpuEngineRowsFailure),
}

#[derive(Clone, Debug, PartialEq)]
pub struct GpuEngineRowsReady {
    pub request_id: RequestId,
    pub snapshot: GpuEngineRowsSnapshot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GpuEngineRowsFailed {
    pub correlation: RequestCorrelation,
    pub device_id: DeviceId,
    pub failure: GpuEngineRowsRequestFailure,
    pub last_good: Option<GpuEngineRowsReady>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum GpuEngineRowsState {
    #[default]
    Closed,
    Loading {
        correlation: RequestCorrelation,
        device_id: DeviceId,
        last_good: Option<GpuEngineRowsReady>,
    },
    Ready(GpuEngineRowsReady),
    Failed(GpuEngineRowsFailed),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuEngineRowsSession {
    state: GpuEngineRowsState,
}

impl GpuEngineRowsSession {
    #[must_use]
    pub const fn state(&self) -> &GpuEngineRowsState {
        &self.state
    }

    pub fn close(&mut self) {
        self.state = GpuEngineRowsState::Closed;
    }

    #[must_use]
    pub fn begin_attempt(&mut self, device_id: DeviceId) -> RequestAttemptId {
        let attempt = RequestAttemptId::next();
        let last_good = self.last_good_for(&device_id).cloned();
        self.state = GpuEngineRowsState::Loading {
            correlation: RequestCorrelation::Attempt(attempt),
            device_id,
            last_good,
        };
        attempt
    }

    pub fn accept_attempt(&mut self, attempt: RequestAttemptId, request_id: RequestId) -> bool {
        let GpuEngineRowsState::Loading { correlation, .. } = &mut self.state else {
            return false;
        };
        if *correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        *correlation = RequestCorrelation::Request(request_id);
        true
    }

    pub fn reject_attempt(&mut self, attempt: RequestAttemptId, failure: FailureKind) -> bool {
        let GpuEngineRowsState::Loading {
            correlation,
            device_id,
            last_good,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        self.state = GpuEngineRowsState::Failed(GpuEngineRowsFailed {
            correlation: *correlation,
            device_id: device_id.clone(),
            failure: GpuEngineRowsRequestFailure::Submission(failure),
            last_good: last_good.clone(),
        });
        true
    }

    pub fn complete(&mut self, request_id: RequestId, snapshot: GpuEngineRowsSnapshot) -> bool {
        let GpuEngineRowsState::Loading {
            correlation,
            device_id,
            last_good,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Request(request_id)
            || *device_id != snapshot.device_id
        {
            return false;
        }
        if let Some(failure) = snapshot.failure.clone() {
            self.state = GpuEngineRowsState::Failed(GpuEngineRowsFailed {
                correlation: *correlation,
                device_id: device_id.clone(),
                failure: GpuEngineRowsRequestFailure::Provider(failure),
                last_good: last_good.clone(),
            });
        } else {
            self.state = GpuEngineRowsState::Ready(GpuEngineRowsReady {
                request_id,
                snapshot,
            });
        }
        true
    }

    pub fn fail(&mut self, request_id: RequestId, failure: FailureKind) -> bool {
        let GpuEngineRowsState::Loading {
            correlation,
            device_id,
            last_good,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Request(request_id) {
            return false;
        }
        self.state = GpuEngineRowsState::Failed(GpuEngineRowsFailed {
            correlation: *correlation,
            device_id: device_id.clone(),
            failure: GpuEngineRowsRequestFailure::Submission(failure),
            last_good: last_good.clone(),
        });
        true
    }

    #[must_use]
    pub fn retry(&mut self) -> Option<RequestAttemptId> {
        let GpuEngineRowsState::Failed(failed) = &self.state else {
            return None;
        };
        Some(self.begin_attempt(failed.device_id.clone()))
    }

    fn last_good_for(&self, device_id: &DeviceId) -> Option<&GpuEngineRowsReady> {
        match &self.state {
            GpuEngineRowsState::Ready(ready) if &ready.snapshot.device_id == device_id => {
                Some(ready)
            }
            GpuEngineRowsState::Loading {
                device_id: current,
                last_good,
                ..
            }
            | GpuEngineRowsState::Failed(GpuEngineRowsFailed {
                device_id: current,
                last_good,
                ..
            }) if current == device_id => last_good.as_ref(),
            _ => None,
        }
    }
}
