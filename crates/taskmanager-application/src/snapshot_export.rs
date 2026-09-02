//! Typed snapshot-export intent and lifecycle.
//!
//! Serialization and filesystem publication belong to the app-host worker.
//! Frontends retain one controller plus a non-blocking [`SnapshotExportPort`]
//! and never perform export work on their event/render thread.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use taskmanager_core::{ProcessItem, SystemSnapshot};

use crate::path_contract::is_single_filename;

pub const MAX_SNAPSHOT_EXPORT_ERROR_CHARS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SnapshotExportRequestId(u64);

impl SnapshotExportRequestId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A renderer-neutral export destination. `CurrentDirectory` defers current
/// directory discovery to the host worker; `BasePath` supports an explicit
/// user-selected destination without granting the frontend filesystem I/O.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotExportTarget {
    CurrentDirectory { stem: Arc<str> },
    BasePath(PathBuf),
}

impl SnapshotExportTarget {
    #[must_use]
    pub fn current_directory(stem: impl Into<Arc<str>>) -> Self {
        Self::CurrentDirectory { stem: stem.into() }
    }

    #[must_use]
    pub fn base_path(path: impl Into<PathBuf>) -> Self {
        Self::BasePath(path.into())
    }

    #[must_use]
    pub fn explicit_base(&self) -> Option<&Path> {
        match self {
            Self::CurrentDirectory { .. } => None,
            Self::BasePath(path) => Some(path),
        }
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self {
            Self::CurrentDirectory { stem } => is_single_filename(stem),
            Self::BasePath(_) => true,
        }
    }
}

/// Immutable bounded work envelope handed to the app-host worker.
#[derive(Clone, Debug)]
pub struct SnapshotExportPayload {
    snapshot: Arc<SystemSnapshot>,
    processes: Arc<[ProcessItem]>,
    target: SnapshotExportTarget,
}

impl SnapshotExportPayload {
    #[must_use]
    pub fn new(
        snapshot: SystemSnapshot,
        processes: impl Into<Arc<[ProcessItem]>>,
        target: SnapshotExportTarget,
    ) -> Self {
        Self {
            snapshot: Arc::new(snapshot),
            processes: processes.into(),
            target,
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> &SystemSnapshot {
        &self.snapshot
    }

    #[must_use]
    pub fn processes(&self) -> &[ProcessItem] {
        &self.processes
    }

    #[must_use]
    pub const fn target(&self) -> &SnapshotExportTarget {
        &self.target
    }
}

#[derive(Clone, Debug)]
pub struct SnapshotExportRequest {
    id: SnapshotExportRequestId,
    payload: SnapshotExportPayload,
}

impl SnapshotExportRequest {
    #[must_use]
    pub const fn id(&self) -> SnapshotExportRequestId {
        self.id
    }

    #[must_use]
    pub const fn payload(&self) -> &SnapshotExportPayload {
        &self.payload
    }
}

/// Stable failure partition across admission, worker lifetime and transaction
/// publication. UI code branches on this enum and never parses host detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SnapshotExportErrorKind {
    Backpressure,
    WorkerStopped,
    Inspect,
    Stage,
    Commit,
    Rollback,
}

impl SnapshotExportErrorKind {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Backpressure => "backpressure",
            Self::WorkerStopped => "worker_stopped",
            Self::Inspect => "inspect",
            Self::Stage => "stage",
            Self::Commit => "commit",
            Self::Rollback => "rollback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotExportError {
    kind: SnapshotExportErrorKind,
    detail: Arc<str>,
}

impl SnapshotExportError {
    #[must_use]
    pub fn new(kind: SnapshotExportErrorKind, detail: impl Into<Arc<str>>) -> Self {
        let detail = detail.into();
        let detail = if detail.chars().count() > MAX_SNAPSHOT_EXPORT_ERROR_CHARS {
            Arc::from(
                detail
                    .chars()
                    .take(MAX_SNAPSHOT_EXPORT_ERROR_CHARS)
                    .collect::<String>(),
            )
        } else {
            detail
        };
        Self { kind, detail }
    }

    #[must_use]
    pub const fn kind(&self) -> SnapshotExportErrorKind {
        self.kind
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for SnapshotExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for SnapshotExportError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotExportOutcome {
    Ready { base: Arc<str> },
    Failed(SnapshotExportError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotExportCompletion {
    pub request: SnapshotExportRequestId,
    pub outcome: SnapshotExportOutcome,
}

#[derive(Clone, Debug, Default)]
pub enum SnapshotExportState {
    #[default]
    Closed,
    Queued(SnapshotExportRequest),
    Running(SnapshotExportRequest),
    Ready {
        request: SnapshotExportRequestId,
        base: Arc<str>,
    },
    Failed {
        request: SnapshotExportRequestId,
        error: SnapshotExportError,
    },
}

impl SnapshotExportState {
    #[must_use]
    pub const fn request_id(&self) -> Option<SnapshotExportRequestId> {
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
pub enum SnapshotExportStartError {
    Busy(SnapshotExportRequestId),
    RequestSpaceExhausted,
    InvalidTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotExportDisposition {
    Applied,
    LateIgnored,
    DuplicateIgnored,
}

/// Non-blocking application port implemented by the app-host client.
pub trait SnapshotExportPort {
    fn try_submit(&mut self, request: SnapshotExportRequest) -> Result<(), SnapshotExportError>;
    fn drain(&mut self) -> Vec<SnapshotExportCompletion>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotExportSubmitError {
    Busy(SnapshotExportRequestId),
    RequestSpaceExhausted,
    Rejected(SnapshotExportError),
}

/// Couples a port to the reducer so no frontend can submit work without
/// advancing `Queued -> Running`, or forget to fold a synchronous rejection.
#[derive(Debug)]
pub struct SnapshotExportSession<P> {
    controller: SnapshotExportController,
    port: P,
}

impl<P: SnapshotExportPort> SnapshotExportSession<P> {
    #[must_use]
    pub fn new(port: P) -> Self {
        Self {
            controller: SnapshotExportController::new(),
            port,
        }
    }

    #[must_use]
    pub const fn state(&self) -> &SnapshotExportState {
        self.controller.state()
    }

    pub fn submit(
        &mut self,
        payload: SnapshotExportPayload,
    ) -> Result<SnapshotExportRequestId, SnapshotExportSubmitError> {
        let request = self
            .controller
            .begin(payload)
            .map_err(|error| match error {
                SnapshotExportStartError::Busy(request) => SnapshotExportSubmitError::Busy(request),
                SnapshotExportStartError::RequestSpaceExhausted => {
                    SnapshotExportSubmitError::RequestSpaceExhausted
                }
                SnapshotExportStartError::InvalidTarget => {
                    SnapshotExportSubmitError::Rejected(SnapshotExportError::new(
                        SnapshotExportErrorKind::Inspect,
                        "snapshot export target must be one portable filename component",
                    ))
                }
            })?;
        let id = request.id();
        if let Err(error) = self.port.try_submit(request) {
            let _ = self.controller.fail_submission(id, error.clone());
            return Err(SnapshotExportSubmitError::Rejected(error));
        }
        let _ = self.controller.mark_running(id);
        Ok(id)
    }

    /// Drain every available completion. Returns the number of terminals that
    /// became authoritative; stale and duplicate completions remain inert.
    pub fn drain(&mut self) -> usize {
        self.port
            .drain()
            .into_iter()
            .filter(|completion| {
                self.controller.complete(completion.clone()) == SnapshotExportDisposition::Applied
            })
            .count()
    }

    pub fn close(&mut self) {
        self.controller.close();
    }
}

/// Sole reducer for export request identity and terminal authority.
#[derive(Debug)]
pub struct SnapshotExportController {
    state: SnapshotExportState,
    next_request: Option<u64>,
}

impl Default for SnapshotExportController {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotExportController {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: SnapshotExportState::Closed,
            next_request: Some(1),
        }
    }

    #[must_use]
    pub const fn state(&self) -> &SnapshotExportState {
        &self.state
    }

    pub fn begin(
        &mut self,
        payload: SnapshotExportPayload,
    ) -> Result<SnapshotExportRequest, SnapshotExportStartError> {
        if let Some(request) = self.state.request_id().filter(|_| self.state.is_active()) {
            return Err(SnapshotExportStartError::Busy(request));
        }
        if !payload.target().is_valid() {
            return Err(SnapshotExportStartError::InvalidTarget);
        }
        let Some(next) = self.next_request else {
            return Err(SnapshotExportStartError::RequestSpaceExhausted);
        };
        self.next_request = next.checked_add(1);
        let request = SnapshotExportRequest {
            id: SnapshotExportRequestId(next),
            payload,
        };
        self.state = SnapshotExportState::Queued(request.clone());
        Ok(request)
    }

    pub fn mark_running(&mut self, request: SnapshotExportRequestId) -> SnapshotExportDisposition {
        match &self.state {
            SnapshotExportState::Queued(queued) if queued.id() == request => {
                self.state = SnapshotExportState::Running(queued.clone());
                SnapshotExportDisposition::Applied
            }
            SnapshotExportState::Running(running) if running.id() == request => {
                SnapshotExportDisposition::DuplicateIgnored
            }
            _ => SnapshotExportDisposition::LateIgnored,
        }
    }

    pub fn fail_submission(
        &mut self,
        request: SnapshotExportRequestId,
        error: SnapshotExportError,
    ) -> SnapshotExportDisposition {
        if !matches!(&self.state, SnapshotExportState::Queued(queued) if queued.id() == request) {
            return SnapshotExportDisposition::LateIgnored;
        }
        self.state = SnapshotExportState::Failed { request, error };
        SnapshotExportDisposition::Applied
    }

    pub fn complete(&mut self, completion: SnapshotExportCompletion) -> SnapshotExportDisposition {
        let current = match &self.state {
            SnapshotExportState::Queued(request) | SnapshotExportState::Running(request) => {
                Some(request.id())
            }
            SnapshotExportState::Ready { request, .. }
            | SnapshotExportState::Failed { request, .. } => {
                if *request == completion.request {
                    return SnapshotExportDisposition::DuplicateIgnored;
                }
                None
            }
            SnapshotExportState::Closed => None,
        };
        if current != Some(completion.request) {
            return SnapshotExportDisposition::LateIgnored;
        }
        self.state = match completion.outcome {
            SnapshotExportOutcome::Ready { base } => SnapshotExportState::Ready {
                request: completion.request,
                base,
            },
            SnapshotExportOutcome::Failed(error) => SnapshotExportState::Failed {
                request: completion.request,
                error,
            },
        };
        SnapshotExportDisposition::Applied
    }

    pub fn close(&mut self) {
        self.state = SnapshotExportState::Closed;
    }
}

#[cfg(test)]
#[path = "../tests/headless/snapshot_export.rs"]
mod tests;
