//! Process Properties insights without provider work on the GPUI thread.
//!
//! The responsibility split is deliberate: the application observation port
//! owns correlated requests while the native provider runtime performs bounded
//! blocking collection; `view` consumes immutable typed states and renders
//! the responsive Properties body. The test-only worker exercises the same
//! capacity-one latest-request semantics without a native provider.

use taskmanager_application::ProcessTelemetrySnapshot;

mod view;

pub(crate) use view::render_process_insights;
pub use view::{
    ProcessInsightsLabels, ProcessInsightsLayout, process_insights_capture_fixture,
    process_insights_layout,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessInsightsErrorKind {
    ProcessUnavailable,
    PermissionDenied,
    ProviderUnavailable,
    Unsupported,
    WorkerDisconnected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessInsightsError {
    pub pid: u32,
    pub kind: ProcessInsightsErrorKind,
    pub last_success_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProcessInsightsState {
    Loading { pid: u32 },
    Ready(Box<ProcessTelemetrySnapshot>),
    Error(ProcessInsightsError),
}

/// Borrowed renderer input. The root lifecycle owns request correlation and
/// terminal state; the view receives only the phase payload it can paint.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ProcessInsightsRenderState<'a> {
    Loading,
    Ready(&'a ProcessTelemetrySnapshot),
    Error(&'a ProcessInsightsError),
}

pub(crate) fn state_from_snapshot(snapshot: ProcessTelemetrySnapshot) -> ProcessInsightsState {
    use crate::core::device_state::DeviceStatus;

    let kind = match snapshot.state.status {
        DeviceStatus::Healthy => return ProcessInsightsState::Ready(Box::new(snapshot)),
        DeviceStatus::Stale => ProcessInsightsErrorKind::ProcessUnavailable,
        DeviceStatus::PermissionDenied => ProcessInsightsErrorKind::PermissionDenied,
        DeviceStatus::MissingTool => ProcessInsightsErrorKind::ProviderUnavailable,
        DeviceStatus::Unsupported => ProcessInsightsErrorKind::Unsupported,
    };
    ProcessInsightsState::Error(ProcessInsightsError {
        pid: snapshot.identity.pid,
        kind,
        last_success_ms: snapshot.state.last_success_ms,
    })
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_app/process_insights/tests.rs"]
mod tests;
