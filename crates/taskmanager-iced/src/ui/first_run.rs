//! Optional setup review over the application controller (ADR-040).
//! Startup observes quietly; only the Settings entry opens this surface.
//! Native typed actions never interpret the displayed command strings.

use iced::Length;
use iced::widget::{column, row, scrollable, text};
use taskmanager_application::i18n::t;
use taskmanager_core::core::setup::{SetupScriptAction, SetupScriptInfo};
use taskmanager_shell::presentation::first_run::first_run_failure_key;

use crate::app::Message;
use crate::focus;
use crate::theme;
use taskmanager_theme::tokens;

use super::components::IcedElement;
use super::overlays::bounded_modal_overlay;
use taskmanager_theme::Theme;

use taskmanager_application::first_run::{FirstRunPhase, FirstRunUiState};

/// The first-run dialog's typed button intents, carried by
/// [`crate::app::Message::FirstRun`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirstRunMessage {
    Open,
    /// Dismiss the dialog without side effects.
    Close,
    /// Submit one typed setup-script action (View / Run / Revert / Restart).
    RequestAction(SetupScriptAction),
    /// Open TaskForest documentation through the typed URL-open port.
    OpenDocumentation,
}

/// Render the dialog from the current view state inside the shared modal
/// shell. Honest states: a missing info renders the discovering line (or the
/// typed failure), never a fabricated script; pending actions disable the
/// action row exactly like GPUI's pills.
pub(crate) fn render_first_run(app: &crate::IcedApp) -> IcedElement<'_> {
    let theme = app.theme();
    let state = app.first_run.view();
    let body = match state.info.as_ref() {
        None => discovering_body(theme, state),
        Some(info) => info_body(theme, state, info),
    };
    let body = scrollable(
        column![text(t("first_run.description")), body].spacing(f32::from(tokens::SPACE_12)),
    )
    .height(Length::Fill)
    .width(Length::Fill)
    .into();
    let actions = if state.info.is_some() {
        action_controls(theme, state, state.action_pending())
    } else {
        Vec::new()
    };
    bounded_modal_overlay(app, t("first_run.title"), body, actions)
}

fn discovering_body<'a>(theme_snapshot: &'a Theme, state: &'a FirstRunUiState) -> IcedElement<'a> {
    let muted = theme::muted_text_color(theme_snapshot);
    let danger = crate::theme_binding::color(theme_snapshot.palette().danger);
    let line = if let FirstRunPhase::Failed(kind) = state.phase {
        text(t(first_run_failure_key(kind)))
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
    theme_snapshot: &'a Theme,
    state: &'a FirstRunUiState,
    info: &'a SetupScriptInfo,
) -> IcedElement<'a> {
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
            text(t(first_run_failure_key(kind)))
                .size(f32::from(tokens::FONT_12))
                .color(crate::theme_binding::color(theme_snapshot.palette().danger)),
        );
    }
    if let Some(status_key) = phase_status_key(&state.phase) {
        body = body.push(
            text(status_key)
                .size(f32::from(tokens::FONT_12))
                .color(crate::theme_binding::color(theme_snapshot.palette().accent)),
        );
    }

    body.width(Length::Fill).into()
}

/// One label / value / copy cluster. The copy button rides the live
/// clipboard message ([`Message::CopyTextToClipboard`]); the value is the
/// observed descriptor string, never interpreted or launched here. `row` is
/// the descriptor row's stable focus position (location / run command /
/// revert command).
fn info_row<'a>(
    theme_snapshot: &'a Theme,
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

fn action_controls<'a>(
    theme_snapshot: &'a Theme,
    state: &'a FirstRunUiState,
    pending: bool,
) -> Vec<IcedElement<'a>> {
    let mut actions: Vec<IcedElement<'a>> = Vec::new();
    if pending {
        // In flight: the actions render as inert text (GPUI disables the
        // pills); no message can be submitted from this frame.
        for label in [
            t("first_run.open_docs"),
            t("first_run.view_script"),
            t("first_run.run_setup"),
            t("first_run.revert_setup"),
        ] {
            actions.push(
                text(label)
                    .size(f32::from(tokens::FONT_12))
                    .color(theme::muted_text_color(theme_snapshot))
                    .into(),
            );
        }
        return actions;
    }
    actions.push(action_button(
        theme_snapshot,
        FocusSlot::action(0),
        t("first_run.open_docs").to_owned(),
        Message::FirstRun(FirstRunMessage::OpenDocumentation),
    ));
    actions.push(action_button(
        theme_snapshot,
        FocusSlot::action(1),
        t("first_run.view_script").to_owned(),
        Message::FirstRun(FirstRunMessage::RequestAction(SetupScriptAction::View)),
    ));
    actions.push(action_button(
        theme_snapshot,
        FocusSlot::action(2),
        t("first_run.run_setup").to_owned(),
        Message::FirstRun(FirstRunMessage::RequestAction(SetupScriptAction::Run)),
    ));
    actions.push(action_button(
        theme_snapshot,
        FocusSlot::action(3),
        t("first_run.revert_setup").to_owned(),
        Message::FirstRun(FirstRunMessage::RequestAction(SetupScriptAction::Revert)),
    ));
    if state.phase == FirstRunPhase::RestartRequired {
        actions.push(action_button(
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
        actions.push(action_button(
            theme_snapshot,
            FocusSlot::action(5),
            t("first_run.retry").to_owned(),
            Message::FirstRun(FirstRunMessage::RequestAction(action)),
        ));
    }
    actions
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
    theme_snapshot: &'a Theme,
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

#[cfg(test)]
#[path = "../../tests/gui/ui/first_run_tests.rs"]
mod tests;
