//! Typed one-shot current-window capture intent and lifecycle.
//!
//! Native capture and PNG publication stay behind the app-host port.  A
//! frontend owns only this request session and renders its immutable state.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use taskmanager_platform_contract::{WindowCaptureBackend, WindowCaptureFailureKind};

pub const MAX_WINDOW_CAPTURE_ERROR_CHARS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowCaptureRequestId(u64);

impl WindowCaptureRequestId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Destination for the published PNG.  The current-directory form accepts a
/// single filename; the host resolves the directory and performs the atomic
/// publication off the UI thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowCaptureTarget {
    CurrentDirectory { filename: Arc<str> },
    Path(PathBuf),
}

impl WindowCaptureTarget {
    #[must_use]
    pub fn current_directory(filename: impl Into<Arc<str>>) -> Self {
        Self::CurrentDirectory {
            filename: filename.into(),
        }
    }

    #[must_use]
    pub fn path(path: impl Into<PathBuf>) -> Self {
        Self::Path(path.into())
    }

    #[must_use]
    pub fn explicit_path(&self) -> Option<&Path> {
        match self {
            Self::CurrentDirectory { .. } => None,
            Self::Path(path) => Some(path.as_path()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WindowCaptureRequest {
    id: WindowCaptureRequestId,
    target: WindowCaptureTarget,
}

impl WindowCaptureRequest {
    #[must_use]
    pub const fn id(&self) -> WindowCaptureRequestId {
        self.id
    }

    #[must_use]
    pub const fn target(&self) -> &WindowCaptureTarget {
        &self.target
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowCaptureErrorKind {
    Backpressure,
    WorkerStopped,
    Inspect,
    Stage,
    Commit,
    Native(WindowCaptureFailureKind),
}

impl WindowCaptureErrorKind {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Backpressure => "backpressure",
            Self::WorkerStopped => "worker_stopped",
            Self::Inspect => "inspect",
            Self::Stage => "stage",
            Self::Commit => "commit",
            Self::Native(kind) => kind.code(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowCaptureError {
    kind: WindowCaptureErrorKind,
    detail: Arc<str>,
}

impl WindowCaptureError {
    #[must_use]
    pub fn new(kind: WindowCaptureErrorKind, detail: impl Into<Arc<str>>) -> Self {
        let detail = detail.into();
        let detail = if detail.chars().count() > MAX_WINDOW_CAPTURE_ERROR_CHARS {
            Arc::from(
                detail
                    .chars()
                    .take(MAX_WINDOW_CAPTURE_ERROR_CHARS)
                    .collect::<String>(),
            )
        } else {
            detail
        };
        Self { kind, detail }
    }

    #[must_use]
    pub const fn kind(&self) -> WindowCaptureErrorKind {
        self.kind
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for WindowCaptureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for WindowCaptureError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowCaptureOutcome {
    Ready {
        destination: Arc<str>,
        width: u32,
        height: u32,
        backend: WindowCaptureBackend,
    },
    Failed(WindowCaptureError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowCaptureCompletion {
    pub request: WindowCaptureRequestId,
    pub outcome: WindowCaptureOutcome,
}

#[derive(Clone, Debug, Default)]
pub enum WindowCaptureState {
    #[default]
    Closed,
    Queued(WindowCaptureRequest),
    Running(WindowCaptureRequest),
    Ready {
        request: WindowCaptureRequestId,
        destination: Arc<str>,
        width: u32,
        height: u32,
        backend: WindowCaptureBackend,
    },
    Failed {
        request: WindowCaptureRequestId,
        error: WindowCaptureError,
    },
}

impl WindowCaptureState {
    #[must_use]
    pub const fn request_id(&self) -> Option<WindowCaptureRequestId> {
        match self {
            Self::Closed => None,
            Self::Queued(request) | Self::Running(request) => Some(request.id()),
            Self::Ready { request, .. } | Self::Failed { request, .. } => Some(*request),
        }
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Queued(_) | Self::Running(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCaptureStartError {
    Busy(WindowCaptureRequestId),
    RequestSpaceExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCaptureDisposition {
    Applied,
    LateIgnored,
    DuplicateIgnored,
}

/// Non-blocking port implemented by the app-host composition edge.
pub trait WindowCapturePort {
    fn try_submit(&mut self, request: WindowCaptureRequest) -> Result<(), WindowCaptureError>;
    fn drain(&mut self) -> Vec<WindowCaptureCompletion>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowCaptureSubmitError {
    Busy(WindowCaptureRequestId),
    RequestSpaceExhausted,
    Rejected(WindowCaptureError),
}

#[derive(Debug)]
pub struct WindowCaptureSession<P> {
    controller: WindowCaptureController,
    port: P,
}

impl<P: WindowCapturePort> WindowCaptureSession<P> {
    #[must_use]
    pub fn new(port: P) -> Self {
        Self {
            controller: WindowCaptureController::new(),
            port,
        }
    }

    #[must_use]
    pub const fn state(&self) -> &WindowCaptureState {
        self.controller.state()
    }

    pub fn submit(
        &mut self,
        target: WindowCaptureTarget,
    ) -> Result<WindowCaptureRequestId, WindowCaptureSubmitError> {
        let request = self.controller.begin(target).map_err(|error| match error {
            WindowCaptureStartError::Busy(request) => WindowCaptureSubmitError::Busy(request),
            WindowCaptureStartError::RequestSpaceExhausted => {
                WindowCaptureSubmitError::RequestSpaceExhausted
            }
        })?;
        let id = request.id();
        if let Err(error) = self.port.try_submit(request) {
            let _ = self.controller.fail_submission(id, error.clone());
            return Err(WindowCaptureSubmitError::Rejected(error));
        }
        let _ = self.controller.mark_running(id);
        Ok(id)
    }

    pub fn drain(&mut self) -> usize {
        self.port
            .drain()
            .into_iter()
            .filter(|completion| {
                self.controller.complete(completion.clone()) == WindowCaptureDisposition::Applied
            })
            .count()
    }

    pub fn close(&mut self) {
        self.controller.close();
    }
}

#[derive(Debug)]
pub struct WindowCaptureController {
    state: WindowCaptureState,
    next_request: Option<u64>,
}

impl Default for WindowCaptureController {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowCaptureController {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: WindowCaptureState::Closed,
            next_request: Some(1),
        }
    }

    #[must_use]
    pub const fn state(&self) -> &WindowCaptureState {
        &self.state
    }

    pub fn begin(
        &mut self,
        target: WindowCaptureTarget,
    ) -> Result<WindowCaptureRequest, WindowCaptureStartError> {
        if let Some(request) = self.state.request_id().filter(|_| self.state.is_active()) {
            return Err(WindowCaptureStartError::Busy(request));
        }
        let Some(next) = self.next_request else {
            return Err(WindowCaptureStartError::RequestSpaceExhausted);
        };
        self.next_request = next.checked_add(1);
        let request = WindowCaptureRequest {
            id: WindowCaptureRequestId(next),
            target,
        };
        self.state = WindowCaptureState::Queued(request.clone());
        Ok(request)
    }

    pub fn mark_running(&mut self, request: WindowCaptureRequestId) -> WindowCaptureDisposition {
        match &self.state {
            WindowCaptureState::Queued(queued) if queued.id() == request => {
                self.state = WindowCaptureState::Running(queued.clone());
                WindowCaptureDisposition::Applied
            }
            WindowCaptureState::Running(running) if running.id() == request => {
                WindowCaptureDisposition::DuplicateIgnored
            }
            _ => WindowCaptureDisposition::LateIgnored,
        }
    }

    pub fn fail_submission(
        &mut self,
        request: WindowCaptureRequestId,
        error: WindowCaptureError,
    ) -> WindowCaptureDisposition {
        if !matches!(&self.state, WindowCaptureState::Queued(queued) if queued.id() == request) {
            return WindowCaptureDisposition::LateIgnored;
        }
        self.state = WindowCaptureState::Failed { request, error };
        WindowCaptureDisposition::Applied
    }

    pub fn complete(&mut self, completion: WindowCaptureCompletion) -> WindowCaptureDisposition {
        let current = match &self.state {
            WindowCaptureState::Queued(request) | WindowCaptureState::Running(request) => {
                Some(request.id())
            }
            WindowCaptureState::Ready { request, .. }
            | WindowCaptureState::Failed { request, .. } => {
                if *request == completion.request {
                    return WindowCaptureDisposition::DuplicateIgnored;
                }
                None
            }
            WindowCaptureState::Closed => None,
        };
        if current != Some(completion.request) {
            return WindowCaptureDisposition::LateIgnored;
        }
        self.state = match completion.outcome {
            WindowCaptureOutcome::Ready {
                destination,
                width,
                height,
                backend,
            } => WindowCaptureState::Ready {
                request: completion.request,
                destination,
                width,
                height,
                backend,
            },
            WindowCaptureOutcome::Failed(error) => WindowCaptureState::Failed {
                request: completion.request,
                error,
            },
        };
        WindowCaptureDisposition::Applied
    }

    pub fn close(&mut self) {
        self.state = WindowCaptureState::Closed;
    }
}

#[cfg(test)]
#[path = "../tests/headless/window_capture.rs"]
mod tests;
