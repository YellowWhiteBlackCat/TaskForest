//! Pure application state transitions.

use taskmanager_core::FrozenProcessIdentity;

use crate::{
    AppAction, AppPage, ConfirmationKind, FocusDirection, InteractionEvent, InteractionReduction,
    InteractionState, PendingConfirmation, PlatformEffect, SelectionDirection,
    ServiceControlTarget, SurfaceDismissReason, SurfaceTransition,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppState {
    pub active_page: AppPage,
    pub selected_process: Option<FrozenProcessIdentity>,
    pub interaction: InteractionState,
    /// The service target + action captured for the next gated confirmation.
    /// Set by the frontend at selection time and consumed by
    /// [`AppAction::RequestServiceControl`], mirroring how `selected_process`
    /// feeds [`AppAction::RequestEndTask`].
    pub selected_service_control: Option<ServiceControlTarget>,
}

/// Toolkit-facing work emitted by the reducer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEffect {
    FocusSearch,
    MoveFocus(FocusDirection),
    MoveSelection(SelectionDirection),
    PageChanged(AppPage),
    /// Ask the owning frontend to atomically toggle its local telemetry
    /// scheduler. No native request or second shared pause state is produced.
    ToggleTelemetryPause,
    /// Ask the owning frontend to toggle its per-window device navigator.
    ToggleSidebar,
    /// Ask the owning frontend to show its system-information surface.
    /// The data remains in the frontend's current correlated read model; this
    /// effect carries no platform request and performs no collection.
    ShowSystemAbout,
}

/// At most one UI effect and one platform effect are produced per action.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reduction {
    pub ui: Option<UiEffect>,
    pub platform: Option<PlatformEffect>,
    /// Primary-surface transition produced by the same action. Keeping it
    /// beside UI/platform effects lets focus and accessibility consume the
    /// transition directly instead of reconstructing it from before/after
    /// booleans.
    pub surface: SurfaceTransition,
}

/// Apply an action without touching a toolkit, thread, filesystem, or process.
#[must_use]
pub fn reduce(state: &mut AppState, action: AppAction) -> Reduction {
    match action {
        AppAction::FocusSearch => ui(UiEffect::FocusSearch),
        AppAction::MoveFocus(direction) => ui(UiEffect::MoveFocus(direction)),
        AppAction::MoveSelection(direction) => ui(UiEffect::MoveSelection(direction)),
        AppAction::SelectPage(page) => {
            state.active_page = page;
            let dismissed = state
                .interaction
                .reduce(InteractionEvent::Dismiss(SurfaceDismissReason::PageChanged));
            Reduction {
                ui: Some(UiEffect::PageChanged(page)),
                platform: None,
                surface: dismissed.transition,
            }
        }
        AppAction::Refresh(request) => platform(PlatformEffect::Refresh(request)),
        AppAction::RequestEndTask => match state.selected_process.clone() {
            Some(target) if target.authoritative_start_token().is_some() => {
                interaction(state.interaction.reduce(InteractionEvent::ArmConfirmation(
                    PendingConfirmation::EndTask(target),
                )))
            }
            Some(_) | None => Reduction::default(),
        },
        AppAction::ConfirmEndTask => interaction(
            state
                .interaction
                .reduce(InteractionEvent::Confirm(ConfirmationKind::EndTask)),
        ),
        AppAction::RequestServiceControl => match state.selected_service_control.clone() {
            Some(target) => interaction(state.interaction.reduce(
                InteractionEvent::ArmConfirmation(PendingConfirmation::ServiceControl(target)),
            )),
            None => Reduction::default(),
        },
        AppAction::ConfirmServiceControl => interaction(
            state
                .interaction
                .reduce(InteractionEvent::Confirm(ConfirmationKind::ServiceControl)),
        ),
        AppAction::OpenProperties => match state.selected_process.clone() {
            Some(target) if target.authoritative_start_token().is_some() => interaction(
                state
                    .interaction
                    .reduce(InteractionEvent::OpenProcessProperties(target)),
            ),
            Some(_) | None => Reduction::default(),
        },
        AppAction::OpenSystemAbout => ui(UiEffect::ShowSystemAbout),
        AppAction::DismissOverlay => interaction(
            state
                .interaction
                .reduce(InteractionEvent::Dismiss(SurfaceDismissReason::Cancel)),
        ),
        AppAction::TogglePause => ui(UiEffect::ToggleTelemetryPause),
        AppAction::ToggleSidebar => ui(UiEffect::ToggleSidebar),
        // Clipboard delivery is renderer-owned (each toolkit has its own
        // clipboard seam); the reducer only acknowledges the action.
        AppAction::CopySelectedRow => Reduction::default(),
        // The alerts-management surface is frontend-owned (each shape routes
        // its own page or overlay); the reducer only acknowledges the action,
        // mirroring `CopySelectedRow`.
        AppAction::OpenAlerts => Reduction::default(),
    }
}

fn ui(effect: UiEffect) -> Reduction {
    Reduction {
        ui: Some(effect),
        platform: None,
        surface: SurfaceTransition::Unchanged,
    }
}

fn platform(effect: PlatformEffect) -> Reduction {
    Reduction {
        ui: None,
        platform: Some(effect),
        surface: SurfaceTransition::Unchanged,
    }
}

fn interaction(reduction: InteractionReduction) -> Reduction {
    Reduction {
        ui: None,
        platform: reduction.effect,
        surface: reduction.transition,
    }
}

#[cfg(test)]
#[path = "../tests/headless/application_reducer_tests.rs"]
mod tests;
