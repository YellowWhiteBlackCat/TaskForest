//! Correlated asynchronous lifecycles for service details, log streaming, and operational states.
//!
//! # Service Lifecycle State Transitions
//!
//! Across all four frontends (GPUI, Iced, TUI, Bevy UI), service state management is
//! governed by four canonical operational states:
//!
//! - **Active**: The service is running, accepting connections, and performing work.
//!   Native equivalents: `active (running)`, `active (exited)`, `Started`, `Running`.
//! - **Inactive**: The service is stopped, dormant, or uninstantiated.
//!   Native equivalents: `inactive (dead)`, `Stopped`.
//! - **Failed**: The service terminated abnormally, crashed, or failed during startup/reload.
//!   Native equivalents: `failed`, `crashed`, `Error`.
//! - **Reloading**: The service is actively reloading its dynamic configuration or
//!   transitioning without terminating the process or tearing down active connections.
//!   Native equivalents: `active (reloading)`, `reloading`.
//!
//! ## State Transition Matrix
//!
//! | Current State | Trigger / Action | Next State | Failure Fallback |
//! |:--------------|:-----------------|:-----------|:-----------------|
//! | `Inactive`    | `Start`          | `Active`   | `Failed` (exit error, binary missing) |
//! | `Active`      | `Stop`           | `Inactive` | `Failed` (unclean termination, kill failure) |
//! | `Active`      | `Reload`         | `Reloading`| `Failed` (invalid config, reload abort) |
//! | `Active`      | Crash / Signal   | `Failed`   | N/A |
//! | `Active`      | `Restart`        | `Active`   | `Failed` (teardown or restart error) |
//! | `Reloading`   | Reload Complete  | `Active`   | `Failed` (config rejected, process panic) |
//! | `Reloading`   | `Stop`           | `Inactive` | `Failed` |
//! | `Failed`      | `Start`/`Restart`| `Active`   | `Failed` (underlying issue unresolved) |
//! | `Failed`      | `Stop` / Reset   | `Inactive` | `Failed` |
//!
//! ## Asynchronous Correlation & Frontends
//!
//! 1. **Control Correlation**: Managed by [`LatestServiceControlRequest`](crate::LatestServiceControlRequest),
//!    which tracks `(ControlRequestId, ServiceId, ServiceAction)` to ensure stale or duplicate
//!    platform completions never overwrite newer user intent.
//! 2. **Dependency Projection**: Managed by [`ServiceDependenciesLifecycle`], preserving
//!    `last_good` dependencies across transient reloads, restarts, and temporary failures.
//! 3. **Log Stream Projection**: Managed by [`ServiceLogStreamLifecycle`], retaining logs
//!    across query filter reloads and enabling diagnostics inspection for failed, active,
//!    and inactive services alike.

use std::sync::atomic::{AtomicU64, Ordering};

use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::services::{
    ServiceAction, ServiceDeps, ServiceLogFailure, ServiceLogQuery, ServiceLogStreamSnapshot,
    ServiceLogStreamState, ServiceStatus,
};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::{RequestId, SubmissionErrorKind};

/// Canonical operational lifecycle state of a system service for shared application use.
///
/// While [`ServiceStatus`] provides coarse 3-state classification (`Active`,
/// `Inactive`, `Failed`, plus `Unknown`), the shared application layer models the
/// fine-grained operational lifecycle transitions including transient states like
/// [`ServiceLifecycleState::Reloading`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ServiceLifecycleState {
    /// Service is actively running and processing work.
    Active,
    /// Service is stopped, dormant, or uninstantiated.
    #[default]
    Inactive,
    /// Service terminated abnormally, crashed, or encountered an unrecoverable failure.
    Failed,
    /// Service is actively reloading configuration or transitioning without dropping availability.
    Reloading,
}

impl ServiceLifecycleState {
    /// Returns true if this state represents an active operational service (running or reloading).
    #[must_use]
    pub const fn is_operational(self) -> bool {
        matches!(self, Self::Active | Self::Reloading)
    }

    /// Returns true if the service is currently running.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    /// Returns true if the service is stopped or dormant.
    #[must_use]
    pub const fn is_inactive(self) -> bool {
        matches!(self, Self::Inactive)
    }

    /// Returns true if the service has failed or crashed.
    #[must_use]
    pub const fn is_failed(self) -> bool {
        matches!(self, Self::Failed)
    }

    /// Returns true if the service is reloading configuration.
    #[must_use]
    pub const fn is_reloading(self) -> bool {
        matches!(self, Self::Reloading)
    }

    /// Returns a static string label for the lifecycle state.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Inactive => "Inactive",
            Self::Failed => "Failed",
            Self::Reloading => "Reloading",
        }
    }

    /// Classifies provider active and sub-state strings into the canonical lifecycle state.
    ///
    /// Handles systemd, Windows SCM, launchd, and normalized strings.
    #[must_use]
    pub fn from_provider_states(active_state: &str, sub_state: &str) -> Self {
        let sub = sub_state.trim().to_lowercase();
        if sub == "reloading" {
            return Self::Reloading;
        }
        let active = active_state.trim().to_lowercase();
        match active.as_str() {
            "reloading" => Self::Reloading,
            "active" | "running" | "started" | "activating" => {
                if sub == "failed" || sub == "crashed" {
                    Self::Failed
                } else if sub == "reloading" {
                    Self::Reloading
                } else {
                    Self::Active
                }
            }
            "failed" | "crashed" | "error" => Self::Failed,
            "inactive" | "dead" | "stopped" | "deactivating" => Self::Inactive,
            _ => match sub.as_str() {
                "running" | "started" => Self::Active,
                "failed" | "crashed" => Self::Failed,
                "reloading" => Self::Reloading,
                _ => Self::Inactive,
            },
        }
    }

    /// Checks whether a direct transition from `self` to `target` is a valid lifecycle step.
    #[must_use]
    pub const fn can_transition_to(self, target: Self) -> bool {
        match (self, target) {
            // Identity: staying in the same state is always valid (e.g. heartbeat/poll).
            (Self::Active, Self::Active)
            | (Self::Inactive, Self::Inactive)
            | (Self::Failed, Self::Failed)
            | (Self::Reloading, Self::Reloading) => true,

            // From Inactive: can start into Active, or fail during startup.
            (Self::Inactive, Self::Active | Self::Failed) => true,

            // From Active: can stop (Inactive), reload (Reloading), crash (Failed),
            // or restart (transiently Inactive).
            (Self::Active, Self::Inactive | Self::Reloading | Self::Failed) => true,

            // From Reloading: can finish reload (Active), fail reload (Failed),
            // or be stopped (Inactive).
            (Self::Reloading, Self::Active | Self::Failed | Self::Inactive) => true,

            // From Failed: can be started/restarted (Active), or reset/stopped (Inactive).
            (Self::Failed, Self::Active | Self::Inactive) => true,

            // Inactive cannot directly reload without starting first.
            (Self::Inactive, Self::Reloading) => false,
            // Failed cannot directly reload without recovering/starting first.
            (Self::Failed, Self::Reloading) => false,
        }
    }

    /// Determines which control actions are valid from the current lifecycle state.
    #[must_use]
    pub const fn is_action_applicable(self, action: ServiceAction) -> bool {
        match action {
            ServiceAction::Start => matches!(self, Self::Inactive | Self::Failed),
            ServiceAction::Stop => matches!(self, Self::Active | Self::Reloading | Self::Failed),
            ServiceAction::Restart => true,
            ServiceAction::Enable | ServiceAction::Disable | ServiceAction::ReloadDaemon => true,
        }
    }
}

impl std::fmt::Display for ServiceLifecycleState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<&str> for ServiceLifecycleState {
    fn from(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "reloading" => Self::Reloading,
            "active" | "running" | "activating" | "started" => Self::Active,
            "inactive" | "dead" | "deactivating" | "stopped" => Self::Inactive,
            "failed" | "crashed" | "error" => Self::Failed,
            _ => Self::Inactive,
        }
    }
}

impl From<ServiceStatus> for ServiceLifecycleState {
    fn from(status: ServiceStatus) -> Self {
        match status {
            ServiceStatus::Active => Self::Active,
            ServiceStatus::Inactive | ServiceStatus::Unknown => Self::Inactive,
            ServiceStatus::Failed => Self::Failed,
        }
    }
}

impl From<ServiceLifecycleState> for ServiceStatus {
    fn from(state: ServiceLifecycleState) -> Self {
        match state {
            ServiceLifecycleState::Active | ServiceLifecycleState::Reloading => Self::Active,
            ServiceLifecycleState::Inactive => Self::Inactive,
            ServiceLifecycleState::Failed => Self::Failed,
        }
    }
}

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
