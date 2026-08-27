//! Typed request sessions shared by renderer tracks.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    FailureKind, FrozenProcessIdentity, ProcessBatchIntent, ProcessBatchResult, RequestId,
    SmartSelfTestIntent, StorageDeviceTarget, SubmissionErrorKind,
};

mod gpu_engine_rows;
mod network_escalation;
mod shell_ui_action;

pub use gpu_engine_rows::{
    GpuEngineRowsFailed, GpuEngineRowsReady, GpuEngineRowsRequestFailure, GpuEngineRowsSession,
    GpuEngineRowsState,
};
pub use network_escalation::{
    NetworkEscalationFailed, NetworkEscalationReady, NetworkEscalationSession,
    NetworkEscalationState,
};
pub use shell_ui_action::{
    ShellUiActionFailed, ShellUiActionIntent, ShellUiActionReady, ShellUiActionReceipt,
    ShellUiActionSession, ShellUiActionState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestAttemptId(u64);

impl RequestAttemptId {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let mut value = NEXT.fetch_add(1, Ordering::Relaxed);
        if value == 0 {
            value = NEXT.fetch_add(1, Ordering::Relaxed);
        }
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RequestCorrelation {
    Attempt(RequestAttemptId),
    Request(RequestId),
}

#[must_use]
pub const fn request_submission_failure(kind: SubmissionErrorKind) -> FailureKind {
    match kind {
        SubmissionErrorKind::Busy | SubmissionErrorKind::RuntimeStopped => {
            FailureKind::TemporarilyUnavailable
        }
        SubmissionErrorKind::InvalidRequest => FailureKind::Rejected,
        SubmissionErrorKind::UnsupportedCapability => FailureKind::Unsupported,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessAffinityReady {
    pub request_id: RequestId,
    pub target: FrozenProcessIdentity,
    pub cpus: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ProcessAffinityState {
    #[default]
    Closed,
    Loading {
        correlation: RequestCorrelation,
        target: FrozenProcessIdentity,
        last_good: Option<ProcessAffinityReady>,
    },
    Ready(ProcessAffinityReady),
    Failed {
        correlation: RequestCorrelation,
        target: FrozenProcessIdentity,
        failure: FailureKind,
        last_good: Option<ProcessAffinityReady>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessAffinitySession {
    state: ProcessAffinityState,
}

impl ProcessAffinitySession {
    #[must_use]
    pub const fn state(&self) -> &ProcessAffinityState {
        &self.state
    }

    pub fn close(&mut self) {
        self.state = ProcessAffinityState::Closed;
    }

    #[must_use]
    pub fn begin_attempt(&mut self, target: FrozenProcessIdentity) -> RequestAttemptId {
        let attempt = RequestAttemptId::next();
        let last_good = self.last_good_for(&target).cloned();
        self.state = ProcessAffinityState::Loading {
            correlation: RequestCorrelation::Attempt(attempt),
            target,
            last_good,
        };
        attempt
    }

    pub fn accept_attempt(&mut self, attempt: RequestAttemptId, request_id: RequestId) -> bool {
        let ProcessAffinityState::Loading { correlation, .. } = &mut self.state else {
            return false;
        };
        if *correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        *correlation = RequestCorrelation::Request(request_id);
        true
    }

    pub fn reject_attempt(&mut self, attempt: RequestAttemptId, failure: FailureKind) -> bool {
        let ProcessAffinityState::Loading {
            correlation,
            target,
            last_good,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        self.state = ProcessAffinityState::Failed {
            correlation: *correlation,
            target: target.clone(),
            failure,
            last_good: last_good.clone(),
        };
        true
    }

    pub fn complete(
        &mut self,
        request_id: RequestId,
        target: FrozenProcessIdentity,
        cpus: Vec<u32>,
    ) -> bool {
        let ProcessAffinityState::Loading {
            correlation,
            target: expected,
            ..
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Request(request_id) || *expected != target {
            return false;
        }
        self.state = ProcessAffinityState::Ready(ProcessAffinityReady {
            request_id,
            target,
            cpus,
        });
        true
    }

    pub fn fail(&mut self, request_id: RequestId, failure: FailureKind) -> bool {
        let ProcessAffinityState::Loading {
            correlation,
            target,
            last_good,
        } = &self.state
        else {
            return false;
        };
        if *correlation != RequestCorrelation::Request(request_id) {
            return false;
        }
        self.state = ProcessAffinityState::Failed {
            correlation: *correlation,
            target: target.clone(),
            failure,
            last_good: last_good.clone(),
        };
        true
    }

    #[must_use]
    pub fn retry(&mut self) -> Option<RequestAttemptId> {
        let ProcessAffinityState::Failed { target, .. } = &self.state else {
            return None;
        };
        Some(self.begin_attempt(target.clone()))
    }

    fn last_good_for(&self, target: &FrozenProcessIdentity) -> Option<&ProcessAffinityReady> {
        match &self.state {
            ProcessAffinityState::Ready(ready) if &ready.target == target => Some(ready),
            ProcessAffinityState::Loading {
                target: current,
                last_good,
                ..
            }
            | ProcessAffinityState::Failed {
                target: current,
                last_good,
                ..
            } if current == target => last_good.as_ref(),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessBatchLoading {
    pub correlation: RequestCorrelation,
    pub intent: ProcessBatchIntent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessBatchReady {
    pub request_id: RequestId,
    pub result: ProcessBatchResult,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessBatchFailed {
    pub correlation: RequestCorrelation,
    pub intent: ProcessBatchIntent,
    pub failure: FailureKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ProcessBatchState {
    #[default]
    Idle,
    Loading(Box<ProcessBatchLoading>),
    Ready(Box<ProcessBatchReady>),
    Failed(Box<ProcessBatchFailed>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessBatchSession {
    state: ProcessBatchState,
}

impl ProcessBatchSession {
    #[must_use]
    pub const fn state(&self) -> &ProcessBatchState {
        &self.state
    }

    pub fn close(&mut self) {
        self.state = ProcessBatchState::Idle;
    }

    #[must_use]
    pub fn begin_attempt(&mut self, intent: ProcessBatchIntent) -> RequestAttemptId {
        let attempt = RequestAttemptId::next();
        self.state = ProcessBatchState::Loading(Box::new(ProcessBatchLoading {
            correlation: RequestCorrelation::Attempt(attempt),
            intent,
        }));
        attempt
    }

    pub fn accept_attempt(&mut self, attempt: RequestAttemptId, request_id: RequestId) -> bool {
        let ProcessBatchState::Loading(loading) = &mut self.state else {
            return false;
        };
        if loading.correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        loading.correlation = RequestCorrelation::Request(request_id);
        true
    }

    pub fn reject_attempt(&mut self, attempt: RequestAttemptId, failure: FailureKind) -> bool {
        let ProcessBatchState::Loading(loading) = &self.state else {
            return false;
        };
        if loading.correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        self.state = ProcessBatchState::Failed(Box::new(ProcessBatchFailed {
            correlation: loading.correlation,
            intent: loading.intent.clone(),
            failure,
        }));
        true
    }

    pub fn complete(&mut self, request_id: RequestId, result: ProcessBatchResult) -> bool {
        let ProcessBatchState::Loading(loading) = &self.state else {
            return false;
        };
        if loading.correlation != RequestCorrelation::Request(request_id)
            || loading.intent != result.intent
        {
            return false;
        }
        self.state = ProcessBatchState::Ready(Box::new(ProcessBatchReady { request_id, result }));
        true
    }

    pub fn fail(&mut self, request_id: RequestId, failure: FailureKind) -> bool {
        let ProcessBatchState::Loading(loading) = &self.state else {
            return false;
        };
        if loading.correlation != RequestCorrelation::Request(request_id) {
            return false;
        }
        self.state = ProcessBatchState::Failed(Box::new(ProcessBatchFailed {
            correlation: loading.correlation,
            intent: loading.intent.clone(),
            failure,
        }));
        true
    }

    #[must_use]
    pub fn retry(&mut self) -> Option<RequestAttemptId> {
        let ProcessBatchState::Failed(failed) = &self.state else {
            return None;
        };
        Some(self.begin_attempt(failed.intent.clone()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartSelfTestLoading {
    pub correlation: RequestCorrelation,
    pub intent: SmartSelfTestIntent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartSelfTestReady {
    pub request_id: RequestId,
    pub intent: SmartSelfTestIntent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartSelfTestFailed {
    pub correlation: RequestCorrelation,
    pub intent: SmartSelfTestIntent,
    pub failure: FailureKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SmartSelfTestState {
    #[default]
    Idle,
    Loading(Box<SmartSelfTestLoading>),
    Ready(Box<SmartSelfTestReady>),
    Failed(Box<SmartSelfTestFailed>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SmartSelfTestSession {
    state: SmartSelfTestState,
}

impl SmartSelfTestSession {
    #[must_use]
    pub const fn state(&self) -> &SmartSelfTestState {
        &self.state
    }

    pub fn close(&mut self) {
        self.state = SmartSelfTestState::Idle;
    }

    #[must_use]
    pub fn begin_attempt(&mut self, intent: SmartSelfTestIntent) -> RequestAttemptId {
        let attempt = RequestAttemptId::next();
        self.state = SmartSelfTestState::Loading(Box::new(SmartSelfTestLoading {
            correlation: RequestCorrelation::Attempt(attempt),
            intent,
        }));
        attempt
    }

    pub fn accept_attempt(&mut self, attempt: RequestAttemptId, request_id: RequestId) -> bool {
        let SmartSelfTestState::Loading(loading) = &mut self.state else {
            return false;
        };
        if loading.correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        loading.correlation = RequestCorrelation::Request(request_id);
        true
    }

    pub fn reject_attempt(&mut self, attempt: RequestAttemptId, failure: FailureKind) -> bool {
        let SmartSelfTestState::Loading(loading) = &self.state else {
            return false;
        };
        if loading.correlation != RequestCorrelation::Attempt(attempt) {
            return false;
        }
        self.state = SmartSelfTestState::Failed(Box::new(SmartSelfTestFailed {
            correlation: loading.correlation,
            intent: loading.intent.clone(),
            failure,
        }));
        true
    }

    pub fn complete(&mut self, request_id: RequestId, subject: &StorageDeviceTarget) -> bool {
        let SmartSelfTestState::Loading(loading) = &self.state else {
            return false;
        };
        if loading.correlation != RequestCorrelation::Request(request_id)
            || loading.intent.target() != *subject
        {
            return false;
        }
        self.state = SmartSelfTestState::Ready(Box::new(SmartSelfTestReady {
            request_id,
            intent: loading.intent.clone(),
        }));
        true
    }

    pub fn fail(&mut self, request_id: RequestId, failure: FailureKind) -> bool {
        let SmartSelfTestState::Loading(loading) = &self.state else {
            return false;
        };
        if loading.correlation != RequestCorrelation::Request(request_id) {
            return false;
        }
        self.state = SmartSelfTestState::Failed(Box::new(SmartSelfTestFailed {
            correlation: loading.correlation,
            intent: loading.intent.clone(),
            failure,
        }));
        true
    }

    #[must_use]
    pub fn retry(&mut self) -> Option<RequestAttemptId> {
        let SmartSelfTestState::Failed(failed) = &self.state else {
            return None;
        };
        Some(self.begin_attempt(failed.intent.clone()))
    }
}
