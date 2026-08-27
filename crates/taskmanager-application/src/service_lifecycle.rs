//! Correlated asynchronous lifecycles for service details and log streaming.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::{
    FailureKind, RequestId, ServiceDeps, ServiceId, ServiceLogFailure, ServiceLogQuery,
    ServiceLogStreamSnapshot, ServiceLogStreamState, SubmissionErrorKind,
};

/// One mapping for service observation admission across every frontend track.
#[must_use]
pub const fn service_submission_failure(kind: SubmissionErrorKind) -> FailureKind {
    match kind {
        SubmissionErrorKind::Busy | SubmissionErrorKind::RuntimeStopped => {
            FailureKind::TemporarilyUnavailable
        }
        SubmissionErrorKind::InvalidRequest => FailureKind::Rejected,
        SubmissionErrorKind::UnsupportedCapability => FailureKind::Unsupported,
    }
}

/// Identity allocated before the platform client accepts an attempt and
/// returns a [`RequestId`]. Admission rejection therefore remains correlated
/// without inventing a platform request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ServiceAttemptId(u64);

impl ServiceAttemptId {
    #[must_use]
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let mut value = NEXT.fetch_add(1, Ordering::Relaxed);
        if value == 0 {
            value = NEXT.fetch_add(1, Ordering::Relaxed);
        }
        Self(value)
    }
}

/// Correlation is an admission attempt until accepted, then atomically
/// upgrades to the platform request carried by provider completion events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ServiceRequestCorrelation {
    Attempt(ServiceAttemptId),
    Request(RequestId),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ServiceDependenciesLifecycle {
    #[default]
    Closed,
    #[non_exhaustive]
    Loading {
        correlation: ServiceRequestCorrelation,
        target: ServiceId,
        last_good: Option<ServiceDeps>,
    },
    #[non_exhaustive]
    Ready {
        request_id: RequestId,
        target: ServiceId,
        dependencies: ServiceDeps,
    },
    #[non_exhaustive]
    Failed {
        correlation: ServiceRequestCorrelation,
        target: ServiceId,
        failure: FailureKind,
        last_good: Option<ServiceDeps>,
    },
}

impl ServiceDependenciesLifecycle {
    pub fn close(&mut self) {
        *self = Self::Closed;
    }

    pub fn begin(&mut self, request_id: RequestId, target: ServiceId) {
        let last_good = self.last_good_for(&target).cloned();
        *self = Self::Loading {
            correlation: ServiceRequestCorrelation::Request(request_id),
            target,
            last_good,
        };
    }

    #[must_use]
    pub fn begin_attempt(&mut self, target: ServiceId) -> ServiceAttemptId {
        let attempt_id = ServiceAttemptId::next();
        let last_good = self.last_good_for(&target).cloned();
        *self = Self::Loading {
            correlation: ServiceRequestCorrelation::Attempt(attempt_id),
            target,
            last_good,
        };
        attempt_id
    }

    pub fn accept_attempt(&mut self, attempt_id: ServiceAttemptId, request_id: RequestId) -> bool {
        let Self::Loading { correlation, .. } = self else {
            return false;
        };
        if *correlation != ServiceRequestCorrelation::Attempt(attempt_id) {
            return false;
        }
        *correlation = ServiceRequestCorrelation::Request(request_id);
        true
    }

    pub fn reject_attempt(&mut self, attempt_id: ServiceAttemptId, failure: FailureKind) -> bool {
        let Self::Loading {
            correlation,
            target,
            last_good,
        } = self
        else {
            return false;
        };
        if *correlation != ServiceRequestCorrelation::Attempt(attempt_id) {
            return false;
        }
        *self = Self::Failed {
            correlation: *correlation,
            target: target.clone(),
            failure,
            last_good: last_good.clone(),
        };
        true
    }

    pub fn resolve(
        &mut self,
        request_id: RequestId,
        target: ServiceId,
        dependencies: ServiceDeps,
    ) -> bool {
        if !self.matches_loading(request_id, &target) {
            return false;
        }
        *self = Self::Ready {
            request_id,
            target,
            dependencies,
        };
        true
    }

    pub fn fail(&mut self, request_id: RequestId, target: ServiceId, failure: FailureKind) -> bool {
        if !self.matches_loading(request_id, &target) {
            return false;
        }
        let last_good = self.last_good_for(&target).cloned();
        *self = Self::Failed {
            correlation: ServiceRequestCorrelation::Request(request_id),
            target,
            failure,
            last_good,
        };
        true
    }

    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    #[must_use]
    pub const fn failure(&self) -> Option<FailureKind> {
        match self {
            Self::Failed { failure, .. } => Some(*failure),
            _ => None,
        }
    }

    #[must_use]
    pub fn projected(&self) -> Option<&ServiceDeps> {
        match self {
            Self::Ready { dependencies, .. } => Some(dependencies),
            Self::Loading { last_good, .. } | Self::Failed { last_good, .. } => last_good.as_ref(),
            Self::Closed => None,
        }
    }

    #[must_use]
    pub const fn target(&self) -> Option<&ServiceId> {
        match self {
            Self::Closed => None,
            Self::Loading { target, .. }
            | Self::Ready { target, .. }
            | Self::Failed { target, .. } => Some(target),
        }
    }

    fn matches_loading(&self, request_id: RequestId, target: &ServiceId) -> bool {
        matches!(
            self,
            Self::Loading {
                correlation: ServiceRequestCorrelation::Request(current),
                target: current_target,
                ..
            } if *current == request_id && current_target == target
        )
    }

    fn last_good_for(&self, target: &ServiceId) -> Option<&ServiceDeps> {
        match self {
            Self::Ready {
                target: current,
                dependencies,
                ..
            } if current == target => Some(dependencies),
            Self::Loading {
                target: current,
                last_good,
                ..
            }
            | Self::Failed {
                target: current,
                last_good,
                ..
            } if current == target => last_good.as_ref(),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ServiceLogStreamLifecycle {
    #[default]
    Closed,
    #[non_exhaustive]
    Idle { target: ServiceId },
    #[non_exhaustive]
    Loading {
        correlation: ServiceRequestCorrelation,
        query: ServiceLogQuery,
        last_good: ServiceLogStreamState,
    },
    #[non_exhaustive]
    Ready {
        request_id: RequestId,
        query: ServiceLogQuery,
        state: ServiceLogStreamState,
    },
    #[non_exhaustive]
    Failed {
        correlation: ServiceRequestCorrelation,
        query: ServiceLogQuery,
        failure: ServiceLogFailure,
        last_good: ServiceLogStreamState,
    },
}

impl ServiceLogStreamLifecycle {
    #[must_use]
    pub fn open(target: ServiceId) -> Self {
        Self::Idle { target }
    }

    pub fn close(&mut self) {
        *self = Self::Closed;
    }

    pub fn begin(&mut self, request_id: RequestId, query: ServiceLogQuery) -> bool {
        if self.target() != Some(&query.service_id) {
            return false;
        }
        let last_good = self.last_good_for(&query);
        *self = Self::Loading {
            correlation: ServiceRequestCorrelation::Request(request_id),
            query,
            last_good,
        };
        true
    }

    #[must_use]
    pub fn begin_attempt(&mut self, query: ServiceLogQuery) -> Option<ServiceAttemptId> {
        if self.target() != Some(&query.service_id) {
            return None;
        }
        let attempt_id = ServiceAttemptId::next();
        let last_good = self.last_good_for(&query);
        *self = Self::Loading {
            correlation: ServiceRequestCorrelation::Attempt(attempt_id),
            query,
            last_good,
        };
        Some(attempt_id)
    }

    pub fn accept_attempt(&mut self, attempt_id: ServiceAttemptId, request_id: RequestId) -> bool {
        let Self::Loading { correlation, .. } = self else {
            return false;
        };
        if *correlation != ServiceRequestCorrelation::Attempt(attempt_id) {
            return false;
        }
        *correlation = ServiceRequestCorrelation::Request(request_id);
        true
    }

    pub fn reject_attempt(
        &mut self,
        attempt_id: ServiceAttemptId,
        failure: ServiceLogFailure,
    ) -> bool {
        let Self::Loading {
            correlation,
            query,
            last_good,
        } = self
        else {
            return false;
        };
        if *correlation != ServiceRequestCorrelation::Attempt(attempt_id) {
            return false;
        }
        *self = Self::Failed {
            correlation: *correlation,
            query: query.clone(),
            failure,
            last_good: last_good.clone(),
        };
        true
    }

    pub fn resolve(&mut self, request: RequestId, snapshot: ServiceLogStreamSnapshot) -> bool {
        let Self::Loading {
            correlation: ServiceRequestCorrelation::Request(request_id),
            query,
            last_good,
        } = self
        else {
            return false;
        };
        if *request_id != request || *query != snapshot.query {
            return false;
        }
        let request_id = *request_id;
        let query = query.clone();
        match snapshot.state {
            ServiceLogStreamState::Unavailable(failure) => {
                *self = Self::Failed {
                    correlation: ServiceRequestCorrelation::Request(request_id),
                    query,
                    failure,
                    last_good: last_good.clone(),
                };
            }
            state => {
                *self = Self::Ready {
                    request_id,
                    query,
                    state,
                };
            }
        }
        true
    }

    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    #[must_use]
    pub const fn target(&self) -> Option<&ServiceId> {
        match self {
            Self::Closed => None,
            Self::Idle { target } => Some(target),
            Self::Loading { query, .. }
            | Self::Ready { query, .. }
            | Self::Failed { query, .. } => Some(&query.service_id),
        }
    }

    #[must_use]
    pub fn projected_state(&self) -> ServiceLogStreamState {
        match self {
            Self::Closed => ServiceLogStreamState::Empty,
            Self::Idle { .. } => ServiceLogStreamState::Loading,
            Self::Ready { state, .. } => state.clone(),
            Self::Loading { last_good, .. } => {
                if matches!(last_good, ServiceLogStreamState::Empty) {
                    ServiceLogStreamState::Loading
                } else {
                    last_good.clone()
                }
            }
            Self::Failed { failure, .. } => ServiceLogStreamState::Unavailable(failure.clone()),
        }
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&ServiceLogFailure> {
        match self {
            Self::Failed { failure, .. } => Some(failure),
            _ => None,
        }
    }

    fn last_good_for(&self, next: &ServiceLogQuery) -> ServiceLogStreamState {
        match self {
            Self::Idle { target } if target == &next.service_id => ServiceLogStreamState::Empty,
            Self::Ready { query, state, .. } if same_log_generation(query, next) => state.clone(),
            Self::Loading {
                query, last_good, ..
            }
            | Self::Failed {
                query, last_good, ..
            } if same_log_generation(query, next) => last_good.clone(),
            _ => ServiceLogStreamState::Empty,
        }
    }
}

fn same_log_generation(current: &ServiceLogQuery, next: &ServiceLogQuery) -> bool {
    current.service_id == next.service_id
        && current.level == next.level
        && current.time == next.time
}
