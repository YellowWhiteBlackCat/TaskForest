//! Renderer-neutral process-control completion correlation (G-01, ADR-027).
//!
//! The shell fold used to drop `EndTaskCompleted` / `SignalCompleted` /
//! `AffinityApplied` / `ResourceLimitsApplied` outcomes, so TUI/Iced only ever
//! saw the request, never the result. This module owns the shared equivalent
//! of GPUI's `complete_process_control` (root/process_control.rs:173-209): a
//! latest-wins correlation between the envelope request id the platform
//! client allocated at submission and the frozen target, fail-closed
//! acceptance of completions (both the request id AND the pid must echo),
//! one typed completion emitted to the batch fold, and the post-completion
//! process-list refresh request GPUI issues.
use super::SystemProjectionStore;
use taskmanager_application::{
    CapabilityId, FailureKind, FrozenProcessIdentity, OperationFailure, ProcessSignal, RequestId,
    ResourceGroupLimitRequest,
};

/// Which renderer-neutral process-control operation a completion reports.
/// Completion events are success signals; correlated failures arrive as
/// [`OperationFailure`]s, so the recorded [`ProcessControlFeedback::result`]
/// is filled from either source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessControlKind {
    EndTask,
    Signal(ProcessSignal),
    /// Neutral suspend/resume projection (§4.0 语义完备律): adapters may
    /// implement the concept via stop/continue signals, but the user concept
    /// is its own vocabulary and never a POSIX signal in a frontend.
    Suspend,
    Resume,
    Affinity(Vec<u32>),
    ResourceLimits(ResourceGroupLimitRequest),
}

/// One accepted process-control completion (or correlated submission failure)
/// emitted by the fold. It is not retained as a second projection mirror;
/// [`FeedbackState`](super::FeedbackState) owns the visible notice lifecycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessControlFeedback {
    /// The identity frozen at submission time (a later refresh cannot
    /// retarget what the feedback describes).
    pub target: FrozenProcessIdentity,
    pub kind: ProcessControlKind,
    pub result: Result<(), FailureKind>,
}

/// One submitted process-control request awaiting its correlated outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingProcessControl {
    pub(crate) request_id: RequestId,
    pub(crate) target: FrozenProcessIdentity,
    pub(crate) kind: ProcessControlKind,
}

/// Latest-wins correlation for process-control completions, keyed by the
/// platform envelope request id echoed in `CorrelatedProcessEvent`.
/// Acceptance is fail-closed like GPUI's `take_matching_process_control`:
/// a completion must echo BOTH the request id and the target pid, so a
/// malformed, stale, or mismatched outcome can never consume an unrelated
/// pending submission.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LatestProcessControlRequest {
    pending: Option<PendingProcessControl>,
}

impl LatestProcessControlRequest {
    pub(crate) fn begin(
        &mut self,
        request_id: RequestId,
        target: FrozenProcessIdentity,
        kind: ProcessControlKind,
    ) {
        self.pending = Some(PendingProcessControl {
            request_id,
            target,
            kind,
        });
    }

    /// Accept only a completion echoing the pending request id AND pid;
    /// removes and returns the pending submission.
    pub(crate) fn accept(
        &mut self,
        request_id: RequestId,
        completed_pid: u32,
    ) -> Option<PendingProcessControl> {
        if !self.pending.as_ref().is_some_and(|pending| {
            pending.request_id == request_id && pending.target.pid == completed_pid
        }) {
            return None;
        }
        self.pending.take()
    }

    /// Remove and return the pending submission when only the request id
    /// matches (failure correlation: an `OperationFailure` carries the
    /// request id but no pid echo).
    pub(crate) fn take(&mut self, request_id: RequestId) -> Option<PendingProcessControl> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.request_id == request_id)
        {
            return self.pending.take();
        }
        None
    }
}

impl SystemProjectionStore {
    /// Record an accepted process-control submission for later completion
    /// correlation (called by `queue_effect`).
    pub fn begin_process_control(
        &mut self,
        request_id: RequestId,
        target: FrozenProcessIdentity,
        kind: ProcessControlKind,
    ) {
        self.process_control_requests
            .begin(request_id, target, kind);
    }

    /// Fold one completed single-target control outcome (EndTask / Signal /
    /// Suspend-Resume / Affinity / ResourceLimits) — the shared semantic of
    /// GPUI's `complete_process_control`: fail-closed correlation, clear the
    /// pending submission, record typed feedback, request a process-list
    /// refresh.
    ///
    /// The recorded kind is the SUBMISSION-time vocabulary, not the event's:
    /// adapters complete `Suspend`/`Resume` requests as `SignalCompleted`
    /// with the platform's stop/continue signal (§4.0 映射穷尽律), and the
    /// feedback must keep describing the concept the user invoked.
    pub(super) fn apply_process_control_completion(
        &mut self,
        request_id: RequestId,
        target: FrozenProcessIdentity,
    ) -> Option<ProcessControlFeedback> {
        let Some(pending) = self.process_control_requests.accept(request_id, target.pid) else {
            // Unknown / superseded / mismatched-pid outcome: change nothing.
            return None;
        };
        let feedback = ProcessControlFeedback {
            target: pending.target,
            kind: pending.kind,
            result: Ok(()),
        };
        self.process_refresh_request = Some(taskmanager_application::RefreshRequest::Processes);
        Some(feedback)
    }

    /// Mirror a correlated submission failure onto the process-control
    /// trackers (mirrors GPUI's `apply_process_control_failure` /
    /// affinity-read session): a failed request never produces a
    /// completion, so its pending entry must not linger and the typed failure
    /// becomes feedback. No refresh is requested — the control did not land.
    pub(super) fn apply_process_control_failure(
        &mut self,
        failure: &OperationFailure,
    ) -> Option<ProcessControlFeedback> {
        // `CapabilityId` constants are `Cow<str>` (not structural-match), so
        // compare like the GPUI failure arms do.
        if (failure.capability == CapabilityId::PROCESS_CONTROL
            || failure.capability == CapabilityId::PROCESS_AFFINITY_CONTROL
            || failure.capability == CapabilityId::PROCESS_RESOURCE_CONTROL)
            && let Some(pending) = self.process_control_requests.take(failure.request_id)
        {
            let feedback = ProcessControlFeedback {
                target: pending.target,
                kind: pending.kind,
                result: Err(failure.kind),
            };
            return Some(feedback);
        }
        None
    }
}

#[cfg(test)]
#[path = "../../tests/headless/shell_app_process_control.rs"]
mod tests;
