//! Toolkit-neutral primary interaction state.
//!
//! Renderer-local widget handles remain in their frontend. This module owns
//! the semantic surface shared by every renderer: at most one process-details
//! surface or dangerous confirmation may be active, and only an explicit,
//! branch-matched confirm transition can produce platform work.

use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessGroupScope,
};
use taskmanager_core::core::session::{SessionControlAction, SessionItem};
use taskmanager_core::core::system_health::SmartSelfTestIntent;

use crate::{
    ControlRequestId, PlatformEffect, ServiceControlTarget, SessionControlTarget,
    SmartControlRequest, StartupControlRequest,
};

/// Stable semantic identity of every shared dangerous confirmation branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConfirmationKind {
    EndTask,
    ProcessTermination,
    ProcessBatch,
    ServiceControl,
    StartupControl,
    SessionControl,
    SmartSelfTest,
}

impl ConfirmationKind {
    /// Single source for architecture tooling and presentation projections.
    pub const ALL: [Self; 7] = [
        Self::EndTask,
        Self::ProcessTermination,
        Self::ProcessBatch,
        Self::ServiceControl,
        Self::StartupControl,
        Self::SessionControl,
        Self::SmartSelfTest,
    ];
}

/// Destructive process action captured by the shared confirmation machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProcessTerminationAction {
    EndTask,
    ForceKill,
    EndProcessTree,
}

/// Frozen GPUI process-termination preview and execution scope.
///
/// GPUI offers richer single/tree verbs than the compact shell surfaces, but
/// the semantic payload still belongs here: names, PIDs and authoritative
/// start tokens cannot live in a renderer-local modal state. Descendants are
/// retained leaf-first and the root is always submitted last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessTerminationConfirmation {
    pub action: ProcessTerminationAction,
    pub root: FrozenProcessIdentity,
    pub descendants_leaf_first: Vec<FrozenProcessIdentity>,
}

impl ProcessTerminationConfirmation {
    #[must_use]
    pub fn descendant_count(&self) -> usize {
        self.descendants_leaf_first.len()
    }

    #[must_use]
    pub fn execution_pids(&self) -> Vec<u32> {
        self.descendants_leaf_first
            .iter()
            .map(|target| target.pid)
            .chain(std::iter::once(self.root.pid))
            .collect()
    }

    fn into_platform_effect(self) -> PlatformEffect {
        match self.action {
            ProcessTerminationAction::EndTask => PlatformEffect::EndTask(self.root),
            ProcessTerminationAction::ForceKill | ProcessTerminationAction::EndProcessTree => {
                let action = match self.action {
                    ProcessTerminationAction::ForceKill => ProcessBatchAction::Kill,
                    ProcessTerminationAction::EndTask
                    | ProcessTerminationAction::EndProcessTree => ProcessBatchAction::End,
                };
                PlatformEffect::ExecuteBatch(ProcessBatchIntent {
                    action,
                    scope: ProcessGroupScope::PidAdjacency,
                    targets: self
                        .descendants_leaf_first
                        .into_iter()
                        .chain(std::iter::once(self.root))
                        .collect(),
                })
            }
        }
    }
}

/// Exact login-session payload frozen while its confirmation is visible.
///
/// The observed session is retained for honest confirmation copy; native
/// dispatch consumes only its provider-issued id, action and request id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionControlConfirmation {
    pub request_id: ControlRequestId,
    pub session: SessionItem,
    pub action: SessionControlAction,
}

/// The one shared dangerous intent awaiting explicit confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingConfirmation {
    EndTask(FrozenProcessIdentity),
    ProcessTermination(ProcessTerminationConfirmation),
    ProcessBatch(ProcessBatchIntent),
    ServiceControl(ServiceControlTarget),
    StartupControl(StartupControlRequest),
    SessionControl(SessionControlConfirmation),
    SmartSelfTest(SmartSelfTestIntent),
}

impl PendingConfirmation {
    #[must_use]
    pub const fn kind(&self) -> ConfirmationKind {
        match self {
            Self::EndTask(_) => ConfirmationKind::EndTask,
            Self::ProcessTermination(_) => ConfirmationKind::ProcessTermination,
            Self::ProcessBatch(_) => ConfirmationKind::ProcessBatch,
            Self::ServiceControl(_) => ConfirmationKind::ServiceControl,
            Self::StartupControl(_) => ConfirmationKind::StartupControl,
            Self::SessionControl(_) => ConfirmationKind::SessionControl,
            Self::SmartSelfTest(_) => ConfirmationKind::SmartSelfTest,
        }
    }

    /// Convert a confirmed frozen intent into its sole platform effect.
    #[must_use]
    fn into_platform_effect(self) -> Option<PlatformEffect> {
        match self {
            Self::EndTask(target) => target
                .authoritative_start_token()
                .map(|_| PlatformEffect::EndTask(target)),
            Self::ProcessTermination(intent) => Some(intent.into_platform_effect()),
            Self::ProcessBatch(intent) => Some(PlatformEffect::ExecuteBatch(intent)),
            Self::ServiceControl(target) => Some(PlatformEffect::ServiceControl(target)),
            Self::StartupControl(request) => Some(PlatformEffect::StartupControl(request)),
            Self::SessionControl(pending) => {
                Some(PlatformEffect::SessionControl(SessionControlTarget {
                    request_id: pending.request_id,
                    session_id: pending.session.id.clone(),
                    action: pending.action,
                }))
            }
            Self::SmartSelfTest(intent) => Some(PlatformEffect::SmartControl(
                SmartControlRequest::StartSelfTest(intent),
            )),
        }
    }
}

/// Stable identity of every shared primary-surface branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceKind {
    ProcessProperties,
    Confirmation(ConfirmationKind),
}

impl SurfaceKind {
    /// Complete branch registry. Adding a confirmation kind must update this
    /// catalog and every exhaustive surface projection in the same change.
    pub const ALL: [Self; 8] = [
        Self::ProcessProperties,
        Self::Confirmation(ConfirmationKind::EndTask),
        Self::Confirmation(ConfirmationKind::ProcessTermination),
        Self::Confirmation(ConfirmationKind::ProcessBatch),
        Self::Confirmation(ConfirmationKind::ServiceControl),
        Self::Confirmation(ConfirmationKind::StartupControl),
        Self::Confirmation(ConfirmationKind::SessionControl),
        Self::Confirmation(ConfirmationKind::SmartSelfTest),
    ];
}

/// Why an active surface closed without submitting platform work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceDismissReason {
    Cancel,
    Escape,
    CloseButton,
    Scrim,
    PageChanged,
    TargetUnavailable,
    Completed,
}

/// Every accepted input to the shared primary-surface machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InteractionEvent {
    OpenProcessProperties(FrozenProcessIdentity),
    ArmConfirmation(PendingConfirmation),
    /// Confirm only the branch the activating control was rendered for. A
    /// stale message naming another branch is rejected without consuming or
    /// submitting the current intent.
    Confirm(ConfirmationKind),
    Dismiss(SurfaceDismissReason),
}

/// Observable transition consumed by focus, accessibility and renderers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SurfaceTransition {
    #[default]
    Unchanged,
    Opened(SurfaceKind),
    Replaced {
        previous: SurfaceKind,
        current: SurfaceKind,
    },
    Confirmed(ConfirmationKind),
    Dismissed {
        surface: SurfaceKind,
        reason: SurfaceDismissReason,
    },
}

/// Result of one interaction event. The effect exists only after a matching
/// Confirm event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionReduction {
    pub transition: SurfaceTransition,
    pub effect: Option<PlatformEffect>,
}

impl InteractionReduction {
    const fn unchanged() -> Self {
        Self {
            transition: SurfaceTransition::Unchanged,
            effect: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PrimarySurface {
    ProcessProperties(FrozenProcessIdentity),
    Confirmation(PendingConfirmation),
}

impl PrimarySurface {
    const fn kind(&self) -> SurfaceKind {
        match self {
            Self::ProcessProperties(_) => SurfaceKind::ProcessProperties,
            Self::Confirmation(pending) => SurfaceKind::Confirmation(pending.kind()),
        }
    }
}

/// Single-owner shared primary-surface state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InteractionState {
    primary: Option<PrimarySurface>,
}

impl InteractionState {
    #[must_use]
    pub const fn kind(&self) -> Option<SurfaceKind> {
        match self.primary.as_ref() {
            Some(surface) => Some(surface.kind()),
            None => None,
        }
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.primary.is_some()
    }

    #[must_use]
    pub const fn process_properties(&self) -> Option<&FrozenProcessIdentity> {
        match self.primary.as_ref() {
            Some(PrimarySurface::ProcessProperties(target)) => Some(target),
            Some(PrimarySurface::Confirmation(_)) | None => None,
        }
    }

    #[must_use]
    pub const fn pending_confirmation(&self) -> Option<&PendingConfirmation> {
        match self.primary.as_ref() {
            Some(PrimarySurface::Confirmation(pending)) => Some(pending),
            Some(PrimarySurface::ProcessProperties(_)) | None => None,
        }
    }

    #[must_use]
    pub const fn confirmation_kind(&self) -> Option<ConfirmationKind> {
        match self.pending_confirmation() {
            Some(pending) => Some(pending.kind()),
            None => None,
        }
    }

    /// Apply exactly one event. The primary surface is private, so callers
    /// cannot bypass replacement, branch matching or dismiss semantics.
    #[must_use]
    pub fn reduce(&mut self, event: InteractionEvent) -> InteractionReduction {
        match event {
            InteractionEvent::OpenProcessProperties(target) => {
                self.open(PrimarySurface::ProcessProperties(target))
            }
            InteractionEvent::ArmConfirmation(pending) => {
                self.open(PrimarySurface::Confirmation(pending))
            }
            InteractionEvent::Confirm(expected) => {
                let Some(PrimarySurface::Confirmation(pending)) = self.primary.as_ref() else {
                    return InteractionReduction::unchanged();
                };
                if pending.kind() != expected {
                    return InteractionReduction::unchanged();
                }
                let Some(PrimarySurface::Confirmation(pending)) = self.primary.take() else {
                    return InteractionReduction::unchanged();
                };
                InteractionReduction {
                    transition: SurfaceTransition::Confirmed(expected),
                    effect: pending.into_platform_effect(),
                }
            }
            InteractionEvent::Dismiss(reason) => {
                let Some(surface) = self.primary.take() else {
                    return InteractionReduction::unchanged();
                };
                InteractionReduction {
                    transition: SurfaceTransition::Dismissed {
                        surface: surface.kind(),
                        reason,
                    },
                    effect: None,
                }
            }
        }
    }

    fn open(&mut self, surface: PrimarySurface) -> InteractionReduction {
        let current = surface.kind();
        let previous = self.primary.replace(surface).map(|value| value.kind());
        InteractionReduction {
            transition: previous.map_or(SurfaceTransition::Opened(current), |previous| {
                SurfaceTransition::Replaced { previous, current }
            }),
            effect: None,
        }
    }
}
