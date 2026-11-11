//! Typed confirmation bars for process-control intents.

use iced::Element;
use iced::widget::{row, text};
use taskforest_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent,
};
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::process_batch_action_label;
use taskmanager_theme::Theme;

use crate::app::{FocusTarget, Message};
use crate::focus;

/// Confirm the pending process termination through the shared shell intent.
pub(in crate::ui) fn confirm_bar<'a>(
    theme_snapshot: &'a Theme,
    target: &FrozenProcessIdentity,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    row![
        text(format!("End {} ({})?", target.name, target.pid)),
        focus::button(
            theme_snapshot,
            FocusTarget::ConfirmEndTask,
            t("common.confirm"),
            Message::ConfirmEndTask,
            true,
        ),
        focus::button(
            theme_snapshot,
            FocusTarget::CancelEndTask,
            t("common.cancel"),
            Message::DismissOverlay,
            false,
        ),
    ]
    .spacing(8)
    .padding(4)
    .into()
}

/// The destructive batch (Kill) confirmation bar — mirrors [`confirm_bar`] but
/// confirms the pending batch intent (`ShellApp::pending_batch`). The target is
/// the intent's single frozen identity (single-select today); if the target
/// vanished before confirmation, only the cancel affordance renders.
pub(in crate::ui) fn confirm_batch_bar<'a>(
    theme_snapshot: &'a Theme,
    intent: &ProcessBatchIntent,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let confirm = focus::button(
        theme_snapshot,
        FocusTarget::ConfirmProcessBatch,
        t("common.confirm"),
        Message::ConfirmProcessBatch,
        true,
    );
    let cancel = focus::button(
        theme_snapshot,
        FocusTarget::CancelEndTask,
        t("common.cancel"),
        Message::DismissOverlay,
        false,
    );
    let action_label = process_batch_action_label(intent.action);
    // Surface the full target scope so a multi-target destructive action reads
    // as a frozen identity set rather than the single first row (mirrors the
    // GPUI confirmation scope).
    let prompt: String = match intent.targets.len() {
        0 => action_label.to_string(),
        1 => {
            let target = &intent.targets[0];
            format!("{action_label} {} ({})?", target.name, target.pid)
        }
        count => {
            let target = if intent.action == ProcessBatchAction::EndProcessTree {
                &intent.targets[count - 1]
            } else {
                &intent.targets[0]
            };
            format!(
                "{action_label} {} ({}) +{} more?",
                target.name,
                target.pid,
                count - 1
            )
        }
    };
    if intent.targets.is_empty() {
        row![cancel].spacing(8).padding(4).into()
    } else {
        row![text(prompt), confirm, cancel]
            .spacing(8)
            .padding(4)
            .into()
    }
}
