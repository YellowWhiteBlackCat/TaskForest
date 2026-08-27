//! Terminal snapshot materialization for projected process insights.
//!
//! These helpers turn the independently-scheduled facet states held by
//! [`super::ProjectedProcessInsights`] into the concrete snapshot values that
//! populate a compatibility `ProcessTelemetrySnapshot`. A `Current` facet
//! passes through unchanged; an `Unavailable` facet becomes a typed degraded
//! snapshot so a partial terminal projection stays renderable.

use super::{ProcessInsightFacetState, ProcessInsightUnavailable};
use taskmanager_core::{
    DeviceState, DeviceStatus, FailureKind, ProcessEnvironment, ProcessGpuSnapshot,
    ProcessIsolation, ProcessNetworkSnapshot, ProcessOpenFiles, ProcessResourceObservations,
    ProcessResourceSnapshot, ProcessThreads, ResourceObservation,
};
use taskmanager_platform_contract::SubmissionErrorKind;

pub(super) fn terminal_network(
    state: &ProcessInsightFacetState<ProcessNetworkSnapshot>,
) -> Option<(ProcessNetworkSnapshot, bool)> {
    match state {
        ProcessInsightFacetState::Pending => None,
        ProcessInsightFacetState::Current(value) => Some((value.clone(), true)),
        ProcessInsightFacetState::Unavailable(reason) => {
            let state = unavailable_state(*reason);
            Some((
                ProcessNetworkSnapshot {
                    state,
                    traffic_state: state,
                    ..ProcessNetworkSnapshot::default()
                },
                false,
            ))
        }
    }
}

pub(super) fn terminal_gpu(
    state: &ProcessInsightFacetState<ProcessGpuSnapshot>,
) -> Option<(ProcessGpuSnapshot, bool)> {
    match state {
        ProcessInsightFacetState::Pending => None,
        ProcessInsightFacetState::Current(value) => Some((value.clone(), true)),
        ProcessInsightFacetState::Unavailable(reason) => Some((
            ProcessGpuSnapshot {
                state: unavailable_state(*reason),
                ..ProcessGpuSnapshot::default()
            },
            false,
        )),
    }
}

pub(super) fn terminal_resources(
    state: &ProcessInsightFacetState<ProcessResourceSnapshot>,
) -> Option<(ProcessResourceSnapshot, bool)> {
    match state {
        ProcessInsightFacetState::Pending => None,
        ProcessInsightFacetState::Current(value) => Some((value.clone(), true)),
        ProcessInsightFacetState::Unavailable(reason) => {
            let failure = unavailable_failure(*reason);
            Some((
                ProcessResourceSnapshot::from_observations(
                    unavailable_state(*reason),
                    ProcessResourceObservations {
                        limits: unavailable_resource(failure),
                        resource_groups: unavailable_resource(failure),
                        memory_usage_bytes: unavailable_resource(failure),
                        memory_limit: unavailable_resource(failure),
                        cpu_time_quota_micros: unavailable_resource(failure),
                        cpu_time_period_micros: unavailable_resource(failure),
                        process_count: unavailable_resource(failure),
                        process_limit: unavailable_resource(failure),
                    },
                    Vec::new(),
                ),
                false,
            ))
        }
    }
}

pub(super) fn terminal_isolation(
    state: &ProcessInsightFacetState<ProcessIsolation>,
) -> Option<(ProcessIsolation, bool)> {
    match state {
        ProcessInsightFacetState::Pending => None,
        ProcessInsightFacetState::Current(value) => Some((value.clone(), true)),
        ProcessInsightFacetState::Unavailable(reason) => Some((
            ProcessIsolation {
                state: unavailable_state(*reason),
                ..ProcessIsolation::default()
            },
            false,
        )),
    }
}

pub(super) fn terminal_threads(
    state: &ProcessInsightFacetState<ProcessThreads>,
) -> (ProcessThreads, bool) {
    match state {
        ProcessInsightFacetState::Pending => (ProcessThreads::default(), false),
        ProcessInsightFacetState::Current(value) => (value.clone(), true),
        ProcessInsightFacetState::Unavailable(reason) => (
            ProcessThreads {
                state: unavailable_state(*reason),
                ..ProcessThreads::default()
            },
            false,
        ),
    }
}

pub(super) fn terminal_open_files(
    state: &ProcessInsightFacetState<ProcessOpenFiles>,
) -> (ProcessOpenFiles, bool) {
    match state {
        ProcessInsightFacetState::Pending => (ProcessOpenFiles::default(), false),
        ProcessInsightFacetState::Current(value) => (value.clone(), true),
        ProcessInsightFacetState::Unavailable(reason) => (
            ProcessOpenFiles {
                state: unavailable_state(*reason),
                ..ProcessOpenFiles::default()
            },
            false,
        ),
    }
}

pub(super) fn terminal_environment(
    state: &ProcessInsightFacetState<ProcessEnvironment>,
) -> (ProcessEnvironment, bool) {
    match state {
        ProcessInsightFacetState::Pending => (ProcessEnvironment::default(), false),
        ProcessInsightFacetState::Current(value) => (value.clone(), true),
        ProcessInsightFacetState::Unavailable(reason) => (
            ProcessEnvironment {
                state: unavailable_state(*reason),
                ..ProcessEnvironment::default()
            },
            false,
        ),
    }
}

const fn unavailable_state(reason: ProcessInsightUnavailable) -> DeviceState {
    let status = match reason {
        ProcessInsightUnavailable::Provider(failure) => DeviceStatus::from_failure(failure),
        ProcessInsightUnavailable::Submission(SubmissionErrorKind::UnsupportedCapability) => {
            DeviceStatus::Unsupported
        }
        ProcessInsightUnavailable::Submission(
            SubmissionErrorKind::Busy
            | SubmissionErrorKind::RuntimeStopped
            | SubmissionErrorKind::InvalidRequest,
        ) => DeviceStatus::Stale,
    };
    DeviceState {
        status,
        last_success_ms: None,
    }
}

fn unavailable_resource<T>(failure: FailureKind) -> ResourceObservation<T> {
    ResourceObservation::unavailable(failure)
}

const fn unavailable_failure(reason: ProcessInsightUnavailable) -> FailureKind {
    match reason {
        ProcessInsightUnavailable::Provider(failure) => failure,
        ProcessInsightUnavailable::Submission(SubmissionErrorKind::Busy)
        | ProcessInsightUnavailable::Submission(SubmissionErrorKind::RuntimeStopped) => {
            FailureKind::TemporarilyUnavailable
        }
        ProcessInsightUnavailable::Submission(SubmissionErrorKind::UnsupportedCapability) => {
            FailureKind::Unsupported
        }
        ProcessInsightUnavailable::Submission(SubmissionErrorKind::InvalidRequest) => {
            FailureKind::Rejected
        }
    }
}

/// Fold the states of facets that responded into one terminal snapshot state.
///
/// Unlike a plain severity rollup, `Unsupported` is deliberately excluded: it
/// marks an optional sub-capability (for example Linux traffic accounting
/// without the escalated backend), and frontends map any non-healthy terminal
/// state to a full error page. Real failures still dominate healthy facets.
pub(super) fn aggregate_usable_state(states: impl IntoIterator<Item = DeviceState>) -> DeviceState {
    let mut strongest_failure = None;
    let mut saw_healthy = false;
    let mut last_success_ms: Option<u64> = None;
    for state in states {
        if state.status == DeviceStatus::Healthy {
            saw_healthy = true;
        } else if state.status != DeviceStatus::Unsupported {
            strongest_failure = Some(stronger_status(
                strongest_failure.unwrap_or(DeviceStatus::Unsupported),
                state.status,
            ));
        }
        if let Some(candidate) = state.last_success_ms {
            last_success_ms =
                Some(last_success_ms.map_or(candidate, |current| current.min(candidate)));
        }
    }
    DeviceState {
        status: strongest_failure.unwrap_or(if saw_healthy {
            DeviceStatus::Healthy
        } else {
            DeviceStatus::Unsupported
        }),
        last_success_ms,
    }
}

const fn stronger_status(left: DeviceStatus, right: DeviceStatus) -> DeviceStatus {
    if left.severity() >= right.severity() {
        left
    } else {
        right
    }
}
