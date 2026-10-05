//! Process Properties insights without provider work on the GPUI thread.
//!
//! The responsibility split is deliberate: the application observation port
//! owns correlated requests while the native provider runtime performs bounded
//! blocking collection; `view` consumes immutable typed states and renders
//! the responsive Properties body. The test-only worker exercises the same
//! capacity-one latest-request semantics without a native provider.

use taskmanager_application::ProjectedProcessInsights;
use taskmanager_core::core::process::ProcessLiveKey;

pub(crate) mod view;

pub use view::ProcessInsightsLabels;
pub(crate) use view::render_process_insights;

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
    pub identity: Option<ProcessLiveKey>,
    pub kind: ProcessInsightsErrorKind,
    pub last_success_ms: Option<u64>,
}

/// Borrowed renderer input. The root lifecycle owns request correlation and
/// terminal state; the view receives only the phase payload it can paint.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ProcessInsightsRenderState<'a> {
    Loading,
    Projection(&'a ProjectedProcessInsights),
    Error(&'a ProcessInsightsError),
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_app/process_insights/tests.rs"]
mod tests;
