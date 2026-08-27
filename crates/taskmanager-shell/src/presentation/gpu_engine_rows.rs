//! One renderer-neutral presentation fold for the on-demand GPU-engine
//! request session and the runtime capability catalog.

use taskmanager_application::{
    CapabilityStatus, DeviceId, FailureKind, GpuEngineMetric, GpuEngineRowsRequestFailure,
    GpuEngineRowsState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuEngineRowsAction {
    Enable,
    Disable,
    Reauthorize,
    Recheck,
    None,
}

#[derive(Debug, PartialEq)]
pub enum GpuEngineRowsPresentation<'a> {
    PermissionRequired,
    Loading,
    Active(&'a [GpuEngineMetric]),
    PermissionDenied,
    MissingDependency,
    AuthorizationUnavailable,
    Unsupported,
    Failed,
}

impl GpuEngineRowsPresentation<'_> {
    #[must_use]
    pub const fn action(&self) -> GpuEngineRowsAction {
        match self {
            Self::PermissionRequired => GpuEngineRowsAction::Enable,
            Self::Loading | Self::Active(_) => GpuEngineRowsAction::Disable,
            Self::PermissionDenied => GpuEngineRowsAction::Reauthorize,
            Self::MissingDependency | Self::AuthorizationUnavailable | Self::Failed => {
                GpuEngineRowsAction::Recheck
            }
            Self::Unsupported => GpuEngineRowsAction::None,
        }
    }

    #[must_use]
    pub const fn message_key(&self) -> Option<&'static str> {
        match self {
            Self::PermissionRequired => Some("gpu.engines_requires_auth"),
            Self::Loading => Some("gpu.engines_authenticating"),
            Self::Active(_) => None,
            Self::PermissionDenied => Some("gpu.engines_permission_denied"),
            Self::MissingDependency => Some("gpu.engines_helper_unavailable"),
            Self::AuthorizationUnavailable => Some("gpu.engines_auth_unavailable"),
            Self::Unsupported => Some("gpu.engines_unsupported"),
            Self::Failed => Some("gpu.engines_failed"),
        }
    }
}

#[must_use]
pub fn present_gpu_engine_rows<'a>(
    state: &'a GpuEngineRowsState,
    device_id: &DeviceId,
    capability_status: Option<CapabilityStatus>,
) -> GpuEngineRowsPresentation<'a> {
    match state {
        GpuEngineRowsState::Loading {
            device_id: target,
            last_good,
            ..
        } if target == device_id => last_good
            .as_ref()
            .map_or(GpuEngineRowsPresentation::Loading, |ready| {
                GpuEngineRowsPresentation::Active(&ready.snapshot.engines)
            }),
        GpuEngineRowsState::Ready(ready) if ready.snapshot.device_id == *device_id => {
            GpuEngineRowsPresentation::Active(&ready.snapshot.engines)
        }
        GpuEngineRowsState::Failed(failed) if failed.device_id == *device_id => {
            presentation_from_failure(match &failed.failure {
                GpuEngineRowsRequestFailure::Submission(kind) => *kind,
                GpuEngineRowsRequestFailure::Provider(failure) => failure.kind,
            })
        }
        GpuEngineRowsState::Closed
        | GpuEngineRowsState::Loading { .. }
        | GpuEngineRowsState::Ready(_)
        | GpuEngineRowsState::Failed(_) => presentation_from_capability(capability_status),
    }
}

const fn presentation_from_capability(
    status: Option<CapabilityStatus>,
) -> GpuEngineRowsPresentation<'static> {
    match status {
        Some(CapabilityStatus::Available | CapabilityStatus::PermissionRequired) => {
            GpuEngineRowsPresentation::PermissionRequired
        }
        Some(CapabilityStatus::Degraded(kind)) => presentation_from_failure(kind),
        Some(CapabilityStatus::MissingDependency) => GpuEngineRowsPresentation::MissingDependency,
        Some(CapabilityStatus::Unsupported) | None => GpuEngineRowsPresentation::Unsupported,
        Some(CapabilityStatus::TemporarilyUnavailable | CapabilityStatus::Stale) => {
            GpuEngineRowsPresentation::AuthorizationUnavailable
        }
    }
}

const fn presentation_from_failure(kind: FailureKind) -> GpuEngineRowsPresentation<'static> {
    match kind {
        FailureKind::RequiresEscalation => GpuEngineRowsPresentation::PermissionRequired,
        FailureKind::PermissionDenied => GpuEngineRowsPresentation::PermissionDenied,
        FailureKind::MissingDependency => GpuEngineRowsPresentation::MissingDependency,
        FailureKind::Unsupported => GpuEngineRowsPresentation::Unsupported,
        FailureKind::TemporarilyUnavailable | FailureKind::TimedOut => {
            GpuEngineRowsPresentation::AuthorizationUnavailable
        }
        FailureKind::IdentityChanged | FailureKind::Rejected | FailureKind::ProviderFault => {
            GpuEngineRowsPresentation::Failed
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/presentation_gpu_engine_rows.rs"]
mod tests;
