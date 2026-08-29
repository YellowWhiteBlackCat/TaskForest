//! Iced process-row context menu.
//!
//! The vocabulary and action semantics match the product surface; geometry,
//! focus, and pointer routing remain Iced-local. Since ICED-007 the panel is
//! a self-owned floating surface mounted by the [`crate::ui::components::
//! Popover`] primitive on the row that opened it — it is never clipped by
//! the table viewport and an outside press dismisses it without activating
//! the surface underneath. A stale pid closes itself by rendering nothing;
//! the action handler also revalidates before submitting control.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_core::core::process::ProcessSignal;
use taskmanager_theme::{Theme, tokens};

use crate::app::{FocusTarget, Message, ProcessMenuAction};
use crate::{focus, theme};

/// The floating action panel for one open process menu. Self-owned (the
/// panel must outlive the frame that built it inside the row's lazy body),
/// so it takes the resolved theme and the process name by value.
pub(super) fn panel(
    theme_snapshot: Theme,
    process_name: String,
    pid: u32,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let process_name = if process_name.trim().is_empty() {
        t("proc.unknown_process").to_owned()
    } else {
        process_name
    };
    let header = row![
        text(t("proc.actions")).size(f32::from(tokens::FONT_13)),
        text(format!("{process_name} · PID {pid}"))
            .size(f32::from(tokens::FONT_12))
            .color(theme::muted_text_color(&theme_snapshot)),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let primary = row![
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuEndTask,
            t("proc.end_task"),
            ProcessMenuAction::EndTask,
            true,
        ),
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuEndTree,
            t("proc.end_process_tree"),
            ProcessMenuAction::EndProcessTree,
            true,
        ),
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuKill,
            t("proc.kill"),
            ProcessMenuAction::Kill,
            true,
        ),
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuSuspend,
            t("proc.suspend"),
            ProcessMenuAction::Suspend,
            false,
        ),
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuResume,
            t("proc.resume"),
            ProcessMenuAction::Resume,
            false,
        ),
    ]
    .spacing(6);

    let signals = row![
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuSignalHangup,
            "Send SIGHUP",
            ProcessMenuAction::Signal(ProcessSignal::Hangup),
            false,
        ),
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuSignalInterrupt,
            "Send SIGINT",
            ProcessMenuAction::Signal(ProcessSignal::Interrupt),
            false,
        ),
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuSignalUser1,
            "Send SIGUSR1",
            ProcessMenuAction::Signal(ProcessSignal::User1),
            false,
        ),
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuSignalUser2,
            "Send SIGUSR2",
            ProcessMenuAction::Signal(ProcessSignal::User2),
            false,
        ),
    ]
    .spacing(6);

    let secondary = row![
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuOpenLocation,
            t("proc.open_location"),
            ProcessMenuAction::OpenLocation,
            false,
        ),
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuSearchOnline,
            t("proc.search_online"),
            ProcessMenuAction::SearchOnline,
            false,
        ),
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuProperties,
            "Properties",
            ProcessMenuAction::Properties,
            false,
        ),
    ]
    .spacing(6);

    let copy = row![
        text(t("common.copy")).size(f32::from(tokens::FONT_12)),
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuCopyName,
            t("menu.copy_name"),
            ProcessMenuAction::CopyName,
            false,
        ),
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuCopyPid,
            t("menu.copy_pid"),
            ProcessMenuAction::CopyPid,
            false,
        ),
        action_button(
            theme_snapshot,
            FocusTarget::ProcessMenuCopyCommandLine,
            t("menu.copy_command_line"),
            ProcessMenuAction::CopyCommandLine,
            false,
        ),
        focus::dynamic_button_owned(
            theme_snapshot,
            FocusTarget::ProcessMenuCopyTsv,
            "TSV".to_owned(),
            Message::CopyProcessTsv,
            false,
        ),
        focus::dynamic_button_owned(
            theme_snapshot,
            FocusTarget::ProcessMenuCopyJson,
            "JSON".to_owned(),
            Message::CopyProcessJson,
            false,
        ),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    let close = focus::dynamic_button_owned(
        theme_snapshot,
        FocusTarget::ProcessMenuClose,
        t("common.cancel").to_owned(),
        Message::CloseProcessRowMenu,
        false,
    );

    container(
        column![header, primary, signals, secondary, copy, close]
            .spacing(6)
            .padding(8),
    )
    .style(move |_| theme::panel_style(&theme_snapshot))
    .width(Length::Shrink)
    .into()
}

fn action_button(
    theme_snapshot: Theme,
    target: FocusTarget,
    label: &'static str,
    action: ProcessMenuAction,
    destructive: bool,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    focus::dynamic_button_owned(
        theme_snapshot,
        target,
        label.to_owned(),
        Message::ProcessMenuAction(action),
        destructive,
    )
}
