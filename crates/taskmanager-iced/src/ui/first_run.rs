//! Iced first-run dialog: the Mission Center-compatible optional-setup dialog
//! (GPUI `first_run` parity).
//!
//! Trigger and persistence follow the GPUI contract exactly: the dialog's
//! authority is the application `first-run.setup` capability — a boot-time
//! `SetupScriptAction::Observe` request, whose `Observed(info)` answer decides
//! visibility. There is deliberately **no** "do not show again" config field
//! anywhere in the stack: the platform-side setup script's *absence* is the
//! persisted done-state (running or reverting it consumes the asset, so the
//! next observation honestly reports `None` and the dialog stays hidden).
//!
//! This module owns the frontend-local view state machine and the renderer.
//! The state is a pure fold over typed events (the same shapes GPUI's
//! `RootView` applies from correlated platform events); submitting the typed
//! requests and applying their answers is composition-owned wiring —
//! `FirstRunMessage` carries the dialog's typed button intents for that lane.

use iced::Length;
use iced::widget::{column, row, text};
use taskmanager_application::i18n::t;
use taskmanager_application::{FailureKind, SetupScriptAction, SetupScriptInfo};

use crate::app::Message;
use crate::focus;
use crate::theme;
use taskmanager_theme::tokens;

use super::components::IcedElement;
use super::overlays::modal_overlay;

/// The upstream first-run dialog's wiki destination. Opening it stays a typed
/// intent ([`FirstRunMessage::OpenWiki`]) routed through the URL-open port at
/// composition; this module never launches a browser command itself.
pub const WIKI_URL: &str = "https://gitlab.com/mission-center-devs/mission-center/-/wikis/home";

/// Dialog phases, mirroring GPUI's `FirstRunPhase` one-to-one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FirstRunPhase {
    #[default]
    Hidden,
    Discovering,
    Available,
    Running,
    Reverting,
    RestartRequired,
    Restarting,
    Failed(FailureKind),
}

/// Frontend-local first-run view state (GPUI `FirstRunUiState` parity).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirstRunUiState {
    pub phase: FirstRunPhase,
    pub info: Option<SetupScriptInfo>,
    pub last_action: Option<SetupScriptAction>,
}

/// Typed events the composition lane folds into [`FirstRunUiState`]. These are
/// the pure halves of GPUI's `apply_first_run_event` / `apply_first_run_failure`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FirstRunEvent {
    /// The boot observation answered. `Some(info)` shows the dialog; `None`
    /// keeps it hidden (the honest no-asset / already-consumed state).
    ObservationCompleted(Option<SetupScriptInfo>),
    /// A typed action was accepted for submission (phase moves to the
    /// action's pending state and `last_action` is retained for Retry).
    ActionSubmitted(SetupScriptAction),
    /// A submitted action completed.
    ActionCompleted(SetupScriptAction),
    /// A submitted action (or the observation) failed with a typed kind.
    ActionFailed {
        action: SetupScriptAction,
        kind: FailureKind,
    },
    /// The dialog was dismissed (Escape / close). Zero side effects: the
    /// phase, info and retry memory stay exactly as they were.
    Dismissed,
}

/// Visibility transition consumed by the composition lane
/// (`app::update::first_run`), which maps it onto the Iced-owned
/// `LocalSurface::FirstRun` slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FirstRunTransition {
    #[default]
    Unchanged,
    Shown,
    Hidden,
}

impl FirstRunUiState {
    /// Whether the dialog is on screen. The surface slot owns actual
    /// visibility; this is the state-machine seam the wiring tests pin.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn visible(&self) -> bool {
        !matches!(self.phase, FirstRunPhase::Hidden)
    }

    /// Whether the action row must be disabled (a request is in flight).
    pub(crate) const fn action_pending(&self) -> bool {
        matches!(
            self.phase,
            FirstRunPhase::Running | FirstRunPhase::Reverting | FirstRunPhase::Restarting
        )
    }

    /// Fold one typed event. Pure: no I/O, no platform submission, no clock.
    /// The transitions mirror GPUI's `apply_first_run_event` /
    /// `apply_first_run_failure` halves exactly.
    pub(crate) fn reduce(&mut self, event: FirstRunEvent) -> FirstRunTransition {
        match event {
            FirstRunEvent::ObservationCompleted(info) => {
                self.info = info;
                let available = self.info.is_some();
                self.phase = if available {
                    FirstRunPhase::Available
                } else {
                    FirstRunPhase::Hidden
                };
                if available {
                    FirstRunTransition::Shown
                } else {
                    FirstRunTransition::Hidden
                }
            }
            FirstRunEvent::ActionSubmitted(action) => {
                self.last_action = Some(action);
                self.phase = match action {
                    SetupScriptAction::Run => FirstRunPhase::Running,
                    SetupScriptAction::Revert => FirstRunPhase::Reverting,
                    SetupScriptAction::Restart => FirstRunPhase::Restarting,
                    SetupScriptAction::Observe => FirstRunPhase::Discovering,
                    SetupScriptAction::View => FirstRunPhase::Available,
                };
                FirstRunTransition::Shown
            }
            FirstRunEvent::ActionCompleted(action) => {
                self.phase = match action {
                    SetupScriptAction::Run => FirstRunPhase::RestartRequired,
                    SetupScriptAction::Revert
                    | SetupScriptAction::View
                    | SetupScriptAction::Observe => FirstRunPhase::Available,
                    SetupScriptAction::Restart => FirstRunPhase::Restarting,
                };
                FirstRunTransition::Shown
            }
            FirstRunEvent::ActionFailed { action, kind } => {
                if action == SetupScriptAction::Observe {
                    // A failed observation is the honest "capability cannot
                    // answer" case: the dialog stays hidden (GPUI parity).
                    self.phase = FirstRunPhase::Hidden;
                    FirstRunTransition::Hidden
                } else {
                    self.phase = FirstRunPhase::Failed(kind);
                    FirstRunTransition::Shown
                }
            }
            // Dismissal is side-effect-free by contract: phase, descriptor and
            // retry memory survive, so reopening shows the same honest state.
            FirstRunEvent::Dismissed => FirstRunTransition::Unchanged,
        }
    }
}

/// The first-run dialog's typed button intents, carried by
/// [`crate::app::Message::FirstRun`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirstRunMessage {
    /// Dismiss the dialog without side effects.
    Close,
    /// Submit one typed setup-script action (View / Run / Revert / Restart).
    RequestAction(SetupScriptAction),
    /// Open the upstream wiki through the typed URL-open port.
    OpenWiki,
}

/// Render the dialog from the current view state inside the shared modal
/// shell. Honest states: a missing info renders the discovering line (or the
/// typed failure), never a fabricated script; pending actions disable the
/// action row exactly like GPUI's pills.
pub(crate) fn render_first_run<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    state: &'a FirstRunUiState,
    appear: f32,
) -> IcedElement<'a> {
    let body = match state.info.as_ref() {
        None => discovering_body(theme_snapshot, state),
        Some(info) => info_body(theme_snapshot, state, info),
    };
    modal_overlay(
        theme_snapshot,
        t("first_run.title"),
        t("first_run.description"),
        body,
        appear,
    )
}

fn discovering_body<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    state: &'a FirstRunUiState,
) -> IcedElement<'a> {
    let muted = theme::muted_text_color(theme_snapshot);
    let danger = theme::color(theme_snapshot.palette().danger);
    let line = if let FirstRunPhase::Failed(kind) = state.phase {
        text(failure_key(kind))
            .size(f32::from(tokens::FONT_13))
            .color(danger)
    } else {
        text(t("first_run.discovering"))
            .size(f32::from(tokens::FONT_13))
            .color(muted)
    };
    column![line]
        .spacing(f32::from(tokens::SPACE_8))
        .width(Length::Fill)
        .into()
}

fn info_body<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    state: &'a FirstRunUiState,
    info: &'a SetupScriptInfo,
) -> IcedElement<'a> {
    let pending = state.action_pending();
    let mut body = column![].spacing(f32::from(tokens::SPACE_12));

    body = body
        .push(info_row(
            theme_snapshot,
            t("first_run.location"),
            info.path.display().to_string(),
            t("first_run.copy_location"),
            0,
        ))
        .push(info_row(
            theme_snapshot,
            t("first_run.run_command"),
            info.run_command.clone(),
            t("first_run.copy_command"),
            1,
        ))
        .push(info_row(
            theme_snapshot,
            t("first_run.revert_command"),
            info.revert_command.clone(),
            t("first_run.copy_revert_command"),
            2,
        ));

    if let FirstRunPhase::Failed(kind) = state.phase {
        body = body.push(
            text(failure_key(kind))
                .size(f32::from(tokens::FONT_12))
                .color(theme::color(theme_snapshot.palette().danger)),
        );
    }
    if let Some(status_key) = phase_status_key(&state.phase) {
        body = body.push(
            text(status_key)
                .size(f32::from(tokens::FONT_12))
                .color(theme::color(theme_snapshot.palette().accent)),
        );
    }

    body = body.push(action_row(theme_snapshot, state, pending));
    body.width(Length::Fill).into()
}

/// One label / value / copy cluster. The copy button rides the live
/// clipboard message ([`Message::CopyTextToClipboard`]); the value is the
/// observed descriptor string, never interpreted or launched here. `row` is
/// the descriptor row's stable focus position (location / run command /
/// revert command).
fn info_row<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    label: &'static str,
    value: String,
    copy_label: &'static str,
    row: u8,
) -> IcedElement<'a> {
    let copy_value = value.clone();
    column![
        text(label)
            .size(f32::from(tokens::FONT_11))
            .color(theme::muted_text_color(theme_snapshot)),
        row![
            text(value)
                .size(f32::from(tokens::FONT_12))
                .width(Length::Fill),
            focus::ghost_button(
                theme_snapshot,
                FocusSlot::copy(row),
                copy_label,
                Message::CopyTextToClipboard {
                    label: copy_label.to_owned(),
                    text: copy_value,
                },
            ),
        ]
        .spacing(f32::from(tokens::SPACE_8))
        .align_y(iced::Alignment::Center),
    ]
    .spacing(f32::from(tokens::SPACE_4))
    .width(Length::Fill)
    .into()
}

fn action_row<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    state: &'a FirstRunUiState,
    pending: bool,
) -> IcedElement<'a> {
    let mut actions = row![].spacing(f32::from(tokens::SPACE_8));
    if pending {
        // In flight: the actions render as inert text (GPUI disables the
        // pills); no message can be submitted from this frame.
        for label in [
            t("first_run.open_wiki"),
            t("first_run.view_script"),
            t("first_run.run_setup"),
            t("first_run.revert_setup"),
        ] {
            actions = actions.push(
                text(label)
                    .size(f32::from(tokens::FONT_12))
                    .color(theme::muted_text_color(theme_snapshot)),
            );
        }
        return actions.into();
    }
    actions = actions.push(action_button(
        theme_snapshot,
        FocusSlot::action(0),
        t("first_run.open_wiki").to_owned(),
        Message::FirstRun(FirstRunMessage::OpenWiki),
    ));
    actions = actions.push(action_button(
        theme_snapshot,
        FocusSlot::action(1),
        t("first_run.view_script").to_owned(),
        Message::FirstRun(FirstRunMessage::RequestAction(SetupScriptAction::View)),
    ));
    actions = actions.push(action_button(
        theme_snapshot,
        FocusSlot::action(2),
        t("first_run.run_setup").to_owned(),
        Message::FirstRun(FirstRunMessage::RequestAction(SetupScriptAction::Run)),
    ));
    actions = actions.push(action_button(
        theme_snapshot,
        FocusSlot::action(3),
        t("first_run.revert_setup").to_owned(),
        Message::FirstRun(FirstRunMessage::RequestAction(SetupScriptAction::Revert)),
    ));
    if state.phase == FirstRunPhase::RestartRequired {
        actions = actions.push(action_button(
            theme_snapshot,
            FocusSlot::action(4),
            t("first_run.restart").to_owned(),
            Message::FirstRun(FirstRunMessage::RequestAction(SetupScriptAction::Restart)),
        ));
    }
    if matches!(state.phase, FirstRunPhase::Failed(_))
        && let Some(action @ (SetupScriptAction::Run | SetupScriptAction::Revert)) =
            state.last_action
    {
        actions = actions.push(action_button(
            theme_snapshot,
            FocusSlot::action(5),
            t("first_run.retry").to_owned(),
            Message::FirstRun(FirstRunMessage::RequestAction(action)),
        ));
    }
    actions.into()
}

/// The dialog's dedicated focus-stop helper: every control maps onto the
/// first-run registry variants ([`crate::app::FocusTarget::FirstRunCopy`] /
/// [`crate::app::FocusTarget::FirstRunAction`]) so each has a stable,
/// collision-free focus identity.
struct FocusSlot;

impl FocusSlot {
    const fn copy(row: u8) -> crate::app::FocusTarget {
        crate::app::FocusTarget::FirstRunCopy(row)
    }

    const fn action(index: u8) -> crate::app::FocusTarget {
        crate::app::FocusTarget::FirstRunAction(index)
    }
}

fn action_button<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    target: crate::app::FocusTarget,
    label: String,
    message: Message,
) -> IcedElement<'a> {
    focus::dynamic_button(theme_snapshot, target, label, message, false)
}

fn phase_status_key(phase: &FirstRunPhase) -> Option<&'static str> {
    match phase {
        FirstRunPhase::Running => Some(t("first_run.running")),
        FirstRunPhase::Reverting => Some(t("first_run.reverting")),
        FirstRunPhase::Restarting => Some(t("first_run.restarting")),
        FirstRunPhase::RestartRequired => Some(t("first_run.restart_required")),
        _ => None,
    }
}

fn failure_key(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::Unsupported => t("first_run.failure_unsupported"),
        FailureKind::PermissionDenied | FailureKind::RequiresEscalation => {
            t("first_run.failure_permission")
        }
        FailureKind::MissingDependency => t("first_run.failure_missing_dependency"),
        FailureKind::TimedOut => t("first_run.failure_timeout"),
        FailureKind::IdentityChanged => t("first_run.failure_identity"),
        FailureKind::TemporarilyUnavailable => t("first_run.failure_unavailable"),
        FailureKind::Rejected => t("first_run.failure_rejected"),
        FailureKind::ProviderFault => t("first_run.failure_provider"),
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/first_run_tests.rs"]
mod tests;
