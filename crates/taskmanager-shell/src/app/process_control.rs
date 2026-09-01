//! Renderer-neutral process-control completion correlation (G-01, ADR-027).
//!
//! The shell fold used to drop `EndTaskCompleted` / `SignalCompleted` /
//! `AffinityApplied` / `ResourceLimitsApplied` outcomes, so TUI/Iced only ever
//! saw the request, never the result. This module owns the shared equivalent
//! of GPUI's `complete_process_control` (root/process_control.rs:173-209): a
//! latest-wins correlation between the envelope request id the platform
//! client allocated at submission and the frozen target, fail-closed
//! acceptance of completions (both the request id AND the complete live row
//! identity must echo),
//! one typed completion emitted to the batch fold, and the post-completion
//! process-list refresh request GPUI issues.
use std::collections::HashSet;

use super::SystemProjectionStore;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessItem, ProcessLiveKey,
    ProcessSignal, descendant_live_keys,
};
use taskmanager_core::core::process_telemetry::ResourceGroupLimitRequest;
use taskmanager_platform_contract::{CapabilityId, CapabilityStatus, OperationFailure, RequestId};

use super::process_rows::ProcessRowId;

/// The scope a process-control action will freeze at the application boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessControlScope {
    Single,
    Tree,
    Batch,
}

/// Shared target/availability projection for every process action surface.
///
/// A row may remain visible while its provider start token is unavailable, but
/// it must not become an actionable request. Explicit capability failures are
/// carried as typed state; an absent descriptor remains an unobserved runtime
/// state and is left to the provider submission boundary to resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessControlAvailability {
    NoSelection,
    IdentityUnavailable,
    CapabilityUnavailable {
        status: Option<CapabilityStatus>,
        scope: ProcessControlScope,
        target_count: usize,
    },
    Ready {
        scope: ProcessControlScope,
        target_count: usize,
    },
}

impl ProcessControlAvailability {
    /// Whether an atomic process-control request may be submitted.
    #[must_use]
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    /// Whether the selected action addresses exactly one individual process.
    #[must_use]
    pub const fn is_single_process(self) -> bool {
        matches!(
            self,
            Self::Ready {
                scope: ProcessControlScope::Single,
                ..
            }
        )
    }

    /// Number of exact live targets represented by the projection.
    #[must_use]
    pub const fn target_count(self) -> usize {
        match self {
            Self::Ready { target_count, .. } => target_count,
            Self::CapabilityUnavailable { target_count, .. } => target_count,
            Self::NoSelection | Self::IdentityUnavailable => 0,
        }
    }

    /// The shared target scope, if a request can be formed.
    #[must_use]
    pub const fn scope(self) -> Option<ProcessControlScope> {
        match self {
            Self::Ready { scope, .. } | Self::CapabilityUnavailable { scope, .. } => Some(scope),
            Self::NoSelection | Self::IdentityUnavailable => None,
        }
    }
}

fn live_selected_count(processes: &[ProcessItem], selected: &[ProcessLiveKey]) -> usize {
    selected
        .iter()
        .copied()
        .filter(|identity| {
            processes
                .iter()
                .any(|process| ProcessLiveKey::from_process(process) == Some(*identity))
        })
        .collect::<HashSet<_>>()
        .len()
}

pub(crate) const fn process_control_capability_allowed(
    capability: Option<CapabilityStatus>,
) -> bool {
    matches!(
        capability,
        None | Some(CapabilityStatus::Available | CapabilityStatus::PermissionRequired)
    )
}

/// Resolve the exact live targets represented by the current selection. This
/// is shared by composed and direct frontend tracks so neither track can grow
/// a second tree-expansion or stale-set rule.
#[must_use]
pub(crate) fn process_control_targets(
    processes: &[ProcessItem],
    active_row: Option<ProcessRowId>,
    selected: &[ProcessLiveKey],
) -> Vec<ProcessLiveKey> {
    let mut targets = match active_row {
        Some(ProcessRowId::Application(root)) => descendant_live_keys(processes, root),
        Some(ProcessRowId::Process(identity)) => {
            if !processes
                .iter()
                .any(|process| ProcessLiveKey::from_process(process) == Some(identity))
            {
                Vec::new()
            } else if selected.is_empty() {
                vec![identity]
            } else {
                selected
                    .iter()
                    .copied()
                    .filter(|selected| {
                        processes
                            .iter()
                            .any(|process| ProcessLiveKey::from_process(process) == Some(*selected))
                    })
                    .collect()
            }
        }
        Some(ProcessRowId::Category(_)) => Vec::new(),
        None => selected
            .iter()
            .copied()
            .filter(|selected| {
                processes
                    .iter()
                    .any(|process| ProcessLiveKey::from_process(process) == Some(*selected))
            })
            .collect(),
    };
    targets.sort_unstable();
    targets.dedup();
    targets
}

/// Freeze one exact process tree through the shared core traversal. Both the
/// composed shell track and the direct GPUI track use this helper so a tree
/// action cannot grow a second target-expansion rule.
#[must_use]
pub(crate) fn process_tree_intent(
    processes: &[ProcessItem],
    root: ProcessLiveKey,
    action: ProcessBatchAction,
) -> Option<ProcessBatchIntent> {
    if !processes
        .iter()
        .any(|process| ProcessLiveKey::from_process(process) == Some(root))
    {
        return None;
    }
    let intent = ProcessBatchIntent::freeze_tree(processes, root, action);
    intent
        .targets
        .iter()
        .any(|target| target.live_key() == Some(root))
        .then_some(intent)
}

/// Freeze one atomic batch intent from the shared process-control target
/// projection. Application rows keep their leaf-first tree order; other rows
/// use the exact live target set.
#[must_use]
pub(crate) fn process_control_intent(
    processes: &[ProcessItem],
    active_row: Option<ProcessRowId>,
    selected: &[ProcessLiveKey],
    action: ProcessBatchAction,
) -> Option<ProcessBatchIntent> {
    // An application row always expands to its frozen descendant tree. Keep
    // the semantic verb aligned with that target scope even when a caller
    // arrives through the generic End command (Delete/action bar); otherwise a
    // tree request would execute correctly but be presented as a single-task
    // action.
    let action = if matches!(active_row, Some(ProcessRowId::Application(_)))
        && action == ProcessBatchAction::End
    {
        ProcessBatchAction::EndProcessTree
    } else {
        action
    };
    let intent = match active_row {
        Some(ProcessRowId::Application(root)) => {
            ProcessBatchIntent::freeze_tree(processes, root, action)
        }
        Some(ProcessRowId::Process(_) | ProcessRowId::Category(_)) | None => {
            ProcessBatchIntent::freeze(
                processes,
                process_control_targets(processes, active_row, selected),
                action,
            )
        }
    };
    (!intent.targets.is_empty()).then_some(intent)
}

/// Resolve one selected process row and selection set into the shared control
/// availability state. This is pure and is the only place that decides
/// whether a row is single-target, tree-target, batch-target, or unavailable.
#[must_use]
pub fn process_control_availability(
    processes: &[ProcessItem],
    active_row: Option<ProcessRowId>,
    selected_identities: &[ProcessLiveKey],
    capability: Option<CapabilityStatus>,
) -> ProcessControlAvailability {
    let target = match active_row {
        Some(ProcessRowId::Application(root)) => {
            let count = descendant_live_keys(processes, root).len();
            (count > 0).then_some((ProcessControlScope::Tree, count))
        }
        Some(ProcessRowId::Process(identity)) => {
            let live = processes
                .iter()
                .any(|process| ProcessLiveKey::from_process(process) == Some(identity));
            if !live {
                return ProcessControlAvailability::IdentityUnavailable;
            }
            if selected_identities.is_empty() {
                Some((ProcessControlScope::Single, 1))
            } else {
                match live_selected_count(processes, selected_identities) {
                    0 => return ProcessControlAvailability::IdentityUnavailable,
                    1 => Some((ProcessControlScope::Single, 1)),
                    count => Some((ProcessControlScope::Batch, count)),
                }
            }
        }
        Some(ProcessRowId::Category(_)) => None,
        None => {
            let count = live_selected_count(processes, selected_identities);
            match count {
                0 if selected_identities.is_empty() => None,
                0 => return ProcessControlAvailability::IdentityUnavailable,
                1 => Some((ProcessControlScope::Single, 1)),
                count => Some((ProcessControlScope::Batch, count)),
            }
        }
    };
    let Some((scope, target_count)) = target else {
        return ProcessControlAvailability::NoSelection;
    };
    if !process_control_capability_allowed(capability) {
        return ProcessControlAvailability::CapabilityUnavailable {
            status: capability,
            scope,
            target_count,
        };
    }
    ProcessControlAvailability::Ready {
        scope,
        target_count,
    }
}

/// Which renderer-neutral process-control operation a completion reports.
/// Completion events are success signals; correlated failures arrive as
/// [`OperationFailure`]s, so the recorded [`ProcessControlFeedback::result`]
/// is filled from either source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessControlKind {
    EndTask,
    Signal(ProcessSignal),
    /// Neutral suspend/resume projection (§8.1 语义完备律): adapters may
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
/// a completion must echo BOTH the request id and the target's complete live
/// row identity, so a malformed, stale, or PID-reuse outcome can never consume
/// an unrelated pending submission.
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

    /// Accept only a completion echoing the pending request id AND the exact
    /// live row identity (PID plus provider start token); removes and returns
    /// the pending submission.
    pub(crate) fn accept(
        &mut self,
        request_id: RequestId,
        completed_target: &FrozenProcessIdentity,
    ) -> Option<PendingProcessControl> {
        if !self.pending.as_ref().is_some_and(|pending| {
            pending.request_id == request_id
                && same_live_identity(&pending.target, completed_target)
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

fn same_live_identity(left: &FrozenProcessIdentity, right: &FrozenProcessIdentity) -> bool {
    let Some(left_token) = left.authoritative_start_token() else {
        return false;
    };
    let Some(right_token) = right.authoritative_start_token() else {
        return false;
    };
    let Some(left) = ProcessLiveKey::from_parts(left.pid, left_token) else {
        return false;
    };
    let Some(right) = ProcessLiveKey::from_parts(right.pid, right_token) else {
        return false;
    };
    left == right
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
    /// with the platform's stop/continue signal (§8.1 映射穷尽律), and the
    /// feedback must keep describing the concept the user invoked.
    pub(super) fn apply_process_control_completion(
        &mut self,
        request_id: RequestId,
        target: FrozenProcessIdentity,
    ) -> Option<ProcessControlFeedback> {
        let Some(pending) = self.process_control_requests.accept(request_id, &target) else {
            // Unknown / superseded / mismatched live identity outcome: change
            // nothing.
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
