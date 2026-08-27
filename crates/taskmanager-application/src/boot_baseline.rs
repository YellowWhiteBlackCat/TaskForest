//! Typed request lifecycle for persistent boot-comparison evidence.

use std::fmt;
use std::sync::Arc;

use taskmanager_core::BootTimeline;

const MAX_BOOT_BASELINE_ERROR_CHARS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BootBaselineRequestId(u64);

impl BootBaselineRequestId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootBaselineRequest {
    id: BootBaselineRequestId,
    timeline: Arc<BootTimeline>,
    recorded_at_ms: u64,
}

impl BootBaselineRequest {
    #[must_use]
    pub const fn id(&self) -> BootBaselineRequestId {
        self.id
    }

    #[must_use]
    pub fn timeline(&self) -> &BootTimeline {
        &self.timeline
    }

    #[must_use]
    pub const fn recorded_at_ms(&self) -> u64 {
        self.recorded_at_ms
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootBaselineRecordKind {
    SameBoot,
    NewBoot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootBaselineErrorKind {
    Read,
    Write,
    Decode,
    ResourceLimit,
    Backpressure,
    WorkerStopped,
}

impl BootBaselineErrorKind {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Decode => "decode",
            Self::ResourceLimit => "resource_limit",
            Self::Backpressure => "backpressure",
            Self::WorkerStopped => "worker_stopped",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootBaselineError {
    kind: BootBaselineErrorKind,
    detail: Arc<str>,
}

impl BootBaselineError {
    #[must_use]
    pub fn new(kind: BootBaselineErrorKind, detail: impl Into<Arc<str>>) -> Self {
        let detail = detail.into();
        let detail = if detail.chars().count() > MAX_BOOT_BASELINE_ERROR_CHARS {
            Arc::from(
                detail
                    .chars()
                    .take(MAX_BOOT_BASELINE_ERROR_CHARS)
                    .collect::<String>(),
            )
        } else {
            detail
        };
        Self { kind, detail }
    }

    #[must_use]
    pub const fn kind(&self) -> BootBaselineErrorKind {
        self.kind
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for BootBaselineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.kind.stable_code())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootBaselineCompletionOutcome {
    Recorded {
        kind: BootBaselineRecordKind,
        previous: Option<Arc<BootTimeline>>,
    },
    Failed(BootBaselineError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootBaselineCompletion {
    pub request: BootBaselineRequest,
    pub outcome: BootBaselineCompletionOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootBaselineReady {
    pub request: BootBaselineRequestId,
    pub evidence: Arc<BootTimeline>,
    pub previous: Option<Arc<BootTimeline>>,
    pub kind: BootBaselineRecordKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum BootBaselineState {
    #[default]
    Idle,
    Loading {
        request: BootBaselineRequest,
        last_good: Option<BootBaselineReady>,
    },
    Ready(BootBaselineReady),
    Failed {
        request: BootBaselineRequestId,
        evidence: Arc<BootTimeline>,
        error: BootBaselineError,
        last_good: Option<BootBaselineReady>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootBaselineSubmission {
    Issued(BootBaselineRequest),
    DuplicateIgnored,
    RequestSpaceExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootBaselineCompletionDisposition {
    Applied,
    StaleIgnored,
}

/// Single authority for boot-evidence request identity, duplicate suppression,
/// late completion rejection and last-good comparison retention.
#[derive(Debug)]
pub struct BootBaselineController {
    state: BootBaselineState,
    next_request: Option<u64>,
}

impl Default for BootBaselineController {
    fn default() -> Self {
        Self {
            state: BootBaselineState::Idle,
            next_request: Some(1),
        }
    }
}

impl BootBaselineController {
    #[must_use]
    pub const fn state(&self) -> &BootBaselineState {
        &self.state
    }

    #[must_use]
    pub fn previous_for_current_evidence(&self) -> Option<&BootTimeline> {
        match &self.state {
            BootBaselineState::Ready(ready) => ready.previous.as_deref(),
            BootBaselineState::Loading { request, last_good } => last_good
                .as_ref()
                .filter(|ready| ready.evidence.as_ref() == request.timeline.as_ref())
                .and_then(|ready| ready.previous.as_deref()),
            BootBaselineState::Failed {
                evidence,
                last_good,
                ..
            } => last_good
                .as_ref()
                .filter(|ready| ready.evidence.as_ref() == evidence.as_ref())
                .and_then(|ready| ready.previous.as_deref()),
            BootBaselineState::Idle => None,
        }
    }

    /// Diagnostic last-good comparison retained across a newer failure.
    /// Renderers use [`Self::previous_for_current_evidence`] instead, so an
    /// older boot baseline is never relabeled as belonging to the new boot.
    #[must_use]
    pub fn last_good_previous(&self) -> Option<&BootTimeline> {
        self.last_good().and_then(|ready| ready.previous.as_deref())
    }

    pub fn observe(
        &mut self,
        timeline: BootTimeline,
        recorded_at_ms: u64,
    ) -> BootBaselineSubmission {
        if self.same_successful_or_inflight_evidence(&timeline) {
            return BootBaselineSubmission::DuplicateIgnored;
        }
        let Some(next_request) = self.next_request else {
            return BootBaselineSubmission::RequestSpaceExhausted;
        };
        let request = BootBaselineRequest {
            id: BootBaselineRequestId(next_request),
            timeline: Arc::new(timeline),
            recorded_at_ms,
        };
        let last_good = self.last_good().cloned();
        self.next_request = next_request.checked_add(1);
        self.state = BootBaselineState::Loading {
            request: request.clone(),
            last_good,
        };
        BootBaselineSubmission::Issued(request)
    }

    pub fn reject_submission(
        &mut self,
        request: BootBaselineRequest,
        error: BootBaselineError,
    ) -> BootBaselineCompletionDisposition {
        self.complete(BootBaselineCompletion {
            request,
            outcome: BootBaselineCompletionOutcome::Failed(error),
        })
    }

    pub fn complete(
        &mut self,
        completion: BootBaselineCompletion,
    ) -> BootBaselineCompletionDisposition {
        let (current, last_good) = match &self.state {
            BootBaselineState::Loading { request, last_good } => {
                (request.clone(), last_good.clone())
            }
            BootBaselineState::Idle
            | BootBaselineState::Ready(_)
            | BootBaselineState::Failed { .. } => {
                return BootBaselineCompletionDisposition::StaleIgnored;
            }
        };
        if current.id != completion.request.id {
            return BootBaselineCompletionDisposition::StaleIgnored;
        }
        self.state = match completion.outcome {
            BootBaselineCompletionOutcome::Recorded { kind, previous } => {
                BootBaselineState::Ready(BootBaselineReady {
                    request: current.id,
                    evidence: current.timeline,
                    previous,
                    kind,
                })
            }
            BootBaselineCompletionOutcome::Failed(error) => BootBaselineState::Failed {
                request: current.id,
                evidence: current.timeline,
                error,
                last_good,
            },
        };
        BootBaselineCompletionDisposition::Applied
    }

    fn last_good(&self) -> Option<&BootBaselineReady> {
        match &self.state {
            BootBaselineState::Ready(ready) => Some(ready),
            BootBaselineState::Loading { last_good, .. }
            | BootBaselineState::Failed { last_good, .. } => last_good.as_ref(),
            BootBaselineState::Idle => None,
        }
    }

    fn same_successful_or_inflight_evidence(&self, timeline: &BootTimeline) -> bool {
        match &self.state {
            BootBaselineState::Loading { request, .. } => request.timeline.as_ref() == timeline,
            BootBaselineState::Ready(ready) => ready.evidence.as_ref() == timeline,
            BootBaselineState::Idle | BootBaselineState::Failed { .. } => false,
        }
    }
}
