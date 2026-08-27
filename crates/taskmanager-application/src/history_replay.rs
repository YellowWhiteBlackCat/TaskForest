//! Typed lifecycle and immutable payloads for persistent-history replay.

use std::fmt;
use std::sync::Arc;

use taskmanager_core::{HistorySeriesKey, HistoryWindow};

/// Cross-toolkit replay curve ceiling. The storage query may observe a much
/// longer window; the background owner publishes only this bounded envelope.
pub const MAX_HISTORY_REPLAY_POINTS: usize = 600;
pub const MAX_HISTORY_REPLAY_ERROR_CHARS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HistoryReplayRequestId(u64);

impl HistoryReplayRequestId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryReplayRequest {
    id: HistoryReplayRequestId,
    window: HistoryWindow,
}

impl HistoryReplayRequest {
    #[must_use]
    pub const fn id(self) -> HistoryReplayRequestId {
        self.id
    }

    #[must_use]
    pub const fn window(self) -> HistoryWindow {
        self.window
    }
}

/// One bounded, downsampled persisted series. `Arc` keeps worker publications
/// immutable and cheap to retain as last-good evidence.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryReplayRow {
    pub key: HistorySeriesKey,
    pub samples: Arc<[f32]>,
    /// Completion stamps selected by the exact same decimation positions as
    /// `samples`. Frontends use them to preserve recording downtime gaps.
    pub sample_times_ms: Arc<[u64]>,
    pub peak_value: Option<f64>,
    pub peak_measured_at_ms: Option<u64>,
    pub observed: usize,
    pub gaps: usize,
    pub clock_jumps: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryReplayErrorKind {
    Read,
    Decode,
    ResourceLimit,
    Backpressure,
    WorkerStopped,
}

impl HistoryReplayErrorKind {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Decode => "decode",
            Self::ResourceLimit => "resource_limit",
            Self::Backpressure => "backpressure",
            Self::WorkerStopped => "worker_stopped",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryReplayError {
    kind: HistoryReplayErrorKind,
    detail: Arc<str>,
}

impl HistoryReplayError {
    #[must_use]
    pub fn new(kind: HistoryReplayErrorKind, detail: impl Into<Arc<str>>) -> Self {
        let detail = detail.into();
        let detail = if detail.chars().count() > MAX_HISTORY_REPLAY_ERROR_CHARS {
            Arc::from(
                detail
                    .chars()
                    .take(MAX_HISTORY_REPLAY_ERROR_CHARS)
                    .collect::<String>(),
            )
        } else {
            detail
        };
        Self { kind, detail }
    }

    #[must_use]
    pub const fn kind(&self) -> HistoryReplayErrorKind {
        self.kind
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for HistoryReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.kind.stable_code())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistoryReplayReady {
    pub request: HistoryReplayRequestId,
    pub window: HistoryWindow,
    pub loaded_at_ms: u64,
    pub rows: Arc<[HistoryReplayRow]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum HistoryReplayState {
    #[default]
    Closed,
    Loading {
        request: HistoryReplayRequest,
        last_good: Option<HistoryReplayReady>,
    },
    Ready {
        request: HistoryReplayRequestId,
        window: HistoryWindow,
        loaded_at_ms: u64,
        rows: Arc<[HistoryReplayRow]>,
    },
    Failed {
        request: HistoryReplayRequestId,
        window: HistoryWindow,
        error: HistoryReplayError,
        last_good: Option<HistoryReplayReady>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum HistoryReplayCompletionOutcome {
    Loaded(Arc<[HistoryReplayRow]>),
    Failed(HistoryReplayError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistoryReplayCompletion {
    pub request: HistoryReplayRequest,
    pub loaded_at_ms: u64,
    pub outcome: HistoryReplayCompletionOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryReplayCompletionDisposition {
    Applied,
    StaleIgnored,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryReplayTransitionError {
    Closed,
    AlreadyOpen,
    RequestSpaceExhausted,
}

/// Owns request generation and reduces every lifecycle transition. Renderers
/// keep this controller as their single replay authority; toolkit-only graph
/// caches may project a completed request but never issue or resolve one.
#[derive(Debug)]
pub struct HistoryReplayController {
    state: HistoryReplayState,
    next_request: Option<u64>,
    selected_window: HistoryWindow,
    application_rows_request: Option<HistoryReplayRequestId>,
    application_rows: Arc<[crate::ApplicationHistoryRow]>,
}

impl Default for HistoryReplayController {
    fn default() -> Self {
        Self {
            state: HistoryReplayState::Closed,
            next_request: Some(1),
            selected_window: HistoryWindow::OneHour,
            application_rows_request: None,
            application_rows: Arc::from([]),
        }
    }
}

impl HistoryReplayController {
    #[must_use]
    pub const fn state(&self) -> &HistoryReplayState {
        &self.state
    }

    #[must_use]
    pub const fn selected_window(&self) -> HistoryWindow {
        self.selected_window
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        !matches!(self.state, HistoryReplayState::Closed)
    }

    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self.state, HistoryReplayState::Loading { .. })
    }

    #[must_use]
    pub fn rows(&self) -> &[HistoryReplayRow] {
        match &self.state {
            HistoryReplayState::Ready { rows, .. } => rows,
            HistoryReplayState::Loading {
                last_good: Some(last_good),
                ..
            } => &last_good.rows,
            HistoryReplayState::Failed {
                last_good: Some(last_good),
                ..
            } => &last_good.rows,
            HistoryReplayState::Closed
            | HistoryReplayState::Loading {
                last_good: None, ..
            }
            | HistoryReplayState::Failed {
                last_good: None, ..
            } => &[],
        }
    }

    #[must_use]
    pub const fn rows_request_id(&self) -> Option<HistoryReplayRequestId> {
        match &self.state {
            HistoryReplayState::Ready { request, .. } => Some(*request),
            HistoryReplayState::Loading {
                last_good: Some(last_good),
                ..
            }
            | HistoryReplayState::Failed {
                last_good: Some(last_good),
                ..
            } => Some(last_good.request),
            HistoryReplayState::Closed
            | HistoryReplayState::Loading {
                last_good: None, ..
            }
            | HistoryReplayState::Failed {
                last_good: None, ..
            } => None,
        }
    }

    /// Window that produced the rows returned by [`Self::rows`]. On a failed
    /// wider/narrower refresh this deliberately differs from
    /// [`Self::selected_window`], allowing renderers to label stale evidence
    /// without presenting it as the failed request's result.
    #[must_use]
    pub fn rows_window(&self) -> Option<HistoryWindow> {
        match &self.state {
            HistoryReplayState::Ready { window, .. } => Some(*window),
            HistoryReplayState::Loading {
                last_good: Some(last_good),
                ..
            } => Some(last_good.window),
            HistoryReplayState::Failed {
                last_good: Some(last_good),
                ..
            } => Some(last_good.window),
            HistoryReplayState::Closed
            | HistoryReplayState::Loading {
                last_good: None, ..
            }
            | HistoryReplayState::Failed {
                last_good: None, ..
            } => None,
        }
    }

    #[must_use]
    pub fn loaded_at_ms(&self) -> Option<u64> {
        match &self.state {
            HistoryReplayState::Ready { loaded_at_ms, .. } => Some(*loaded_at_ms),
            HistoryReplayState::Loading {
                last_good: Some(last_good),
                ..
            } => Some(last_good.loaded_at_ms),
            HistoryReplayState::Failed {
                last_good: Some(last_good),
                ..
            } => Some(last_good.loaded_at_ms),
            HistoryReplayState::Closed
            | HistoryReplayState::Loading {
                last_good: None, ..
            }
            | HistoryReplayState::Failed {
                last_good: None, ..
            } => None,
        }
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&HistoryReplayError> {
        match &self.state {
            HistoryReplayState::Failed { error, .. } => Some(error),
            HistoryReplayState::Closed
            | HistoryReplayState::Loading { .. }
            | HistoryReplayState::Ready { .. } => None,
        }
    }

    pub fn open(&mut self) -> Result<HistoryReplayRequest, HistoryReplayTransitionError> {
        if self.is_open() {
            return Err(HistoryReplayTransitionError::AlreadyOpen);
        }
        self.issue(self.selected_window)
    }

    pub fn refresh(&mut self) -> Result<HistoryReplayRequest, HistoryReplayTransitionError> {
        if !self.is_open() {
            return Err(HistoryReplayTransitionError::Closed);
        }
        self.issue(self.selected_window)
    }

    pub fn select_window(
        &mut self,
        window: HistoryWindow,
    ) -> Result<HistoryReplayRequest, HistoryReplayTransitionError> {
        if !self.is_open() {
            return Err(HistoryReplayTransitionError::Closed);
        }
        let request = self.issue(window)?;
        self.selected_window = window;
        Ok(request)
    }

    pub fn close(&mut self) {
        // Closing terminates the replay session: its last-good evidence is
        // owned by the outgoing state and is intentionally discarded. The
        // selected window remains a user preference for the next open.
        self.state = HistoryReplayState::Closed;
        self.sync_application_rows();
    }

    pub fn reject_submission(
        &mut self,
        request: HistoryReplayRequest,
        error: HistoryReplayError,
    ) -> HistoryReplayCompletionDisposition {
        self.complete(HistoryReplayCompletion {
            request,
            loaded_at_ms: 0,
            outcome: HistoryReplayCompletionOutcome::Failed(error),
        })
    }

    pub fn complete(
        &mut self,
        completion: HistoryReplayCompletion,
    ) -> HistoryReplayCompletionDisposition {
        let (request, last_good) = match &self.state {
            HistoryReplayState::Loading { request, last_good } => (*request, last_good.clone()),
            HistoryReplayState::Closed
            | HistoryReplayState::Ready { .. }
            | HistoryReplayState::Failed { .. } => {
                return HistoryReplayCompletionDisposition::StaleIgnored;
            }
        };
        if request.id != completion.request.id || request.window != completion.request.window {
            return HistoryReplayCompletionDisposition::StaleIgnored;
        }
        let request_id = request.id;
        let window = request.window;
        self.state = match completion.outcome {
            HistoryReplayCompletionOutcome::Loaded(rows) => {
                let ready = HistoryReplayReady {
                    request: request_id,
                    window,
                    loaded_at_ms: completion.loaded_at_ms,
                    rows,
                };
                HistoryReplayState::Ready {
                    request: ready.request,
                    window: ready.window,
                    loaded_at_ms: ready.loaded_at_ms,
                    rows: ready.rows,
                }
            }
            HistoryReplayCompletionOutcome::Failed(error) => HistoryReplayState::Failed {
                request: request_id,
                window,
                error,
                last_good,
            },
        };
        self.sync_application_rows();
        HistoryReplayCompletionDisposition::Applied
    }

    /// Durable per-application history derived once for the accepted replay
    /// request. Every frontend consumes this exact read model.
    #[must_use]
    pub fn application_history_projection(
        &self,
        capability: crate::ApplicationHistoryCapability,
    ) -> crate::ApplicationHistoryProjection {
        crate::ApplicationHistoryProjection::from_replay(
            crate::application_history_projection::ApplicationHistoryReplaySnapshot {
                capability,
                selected_window: self.selected_window(),
                rows_window: self.rows_window(),
                rows: Arc::clone(&self.application_rows),
                source_request: self.rows_request_id(),
                refreshing: self.is_loading(),
                failure: self.failure().cloned(),
                loaded_at_ms: self.loaded_at_ms(),
            },
        )
    }

    fn sync_application_rows(&mut self) {
        let request = self.rows_request_id();
        if request == self.application_rows_request {
            return;
        }
        self.application_rows =
            crate::application_history_projection::project_application_history_rows(self.rows());
        self.application_rows_request = request;
    }

    fn issue(
        &mut self,
        window: HistoryWindow,
    ) -> Result<HistoryReplayRequest, HistoryReplayTransitionError> {
        let Some(next_request) = self.next_request else {
            return Err(HistoryReplayTransitionError::RequestSpaceExhausted);
        };
        let request = HistoryReplayRequest {
            id: HistoryReplayRequestId(next_request),
            window,
        };
        let last_good = match &self.state {
            HistoryReplayState::Ready {
                request,
                window,
                loaded_at_ms,
                rows,
            } => Some(HistoryReplayReady {
                request: *request,
                window: *window,
                loaded_at_ms: *loaded_at_ms,
                rows: Arc::clone(rows),
            }),
            HistoryReplayState::Loading { last_good, .. }
            | HistoryReplayState::Failed { last_good, .. } => last_good.clone(),
            HistoryReplayState::Closed => None,
        };
        self.next_request = next_request.checked_add(1);
        self.state = HistoryReplayState::Loading { request, last_good };
        Ok(request)
    }
}
