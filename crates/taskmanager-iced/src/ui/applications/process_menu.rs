//! Iced process-row context menu.
//!
//! This deliberately uses Iced's focusable row/button adapter rather than the
//! GPUI popup implementation. The vocabulary and action semantics match the
//! product surface; geometry, focus, and pointer routing remain Iced-local.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::ProcessSignal;
use taskmanager_application::i18n::t;
use taskmanager_theme::{Theme, tokens};

use crate::app::{FocusTarget, Message, ProcessMenuAction};
use crate::{IcedApp, focus, theme};

/// Render the currently open process menu as a bounded action panel below the
/// process table. A stale pid closes itself by rendering nothing; the action
/// handler also revalidates before submitting control.
pub(super) fn render<'a>(
    app: &'a IcedApp,
    theme_snapshot: &'a Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let Some(pid) = app.process_menu_pid() else {
        return column![].into();
    };
    let Some(process) = app.shell.visible_process_by_pid(pid) else {
        return column![].into();
    };

    let process_name = if process.name.trim().is_empty() {
        t("proc.unknown_process").to_owned()
    } else {
        process.name.clone()
    };
    let header = row![
        text(t("proc.actions")).size(f32::from(tokens::FONT_13)),
        text(format!("{process_name} · PID {pid}"))
            .size(f32::from(tokens::FONT_12))
            .color(theme::muted_text_color(theme_snapshot)),
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
        focus::dynamic_button(
            theme_snapshot,
            FocusTarget::ProcessMenuCopyTsv,
            "TSV".to_string(),
            Message::CopyProcessTsv,
            false,
        ),
        focus::dynamic_button(
            theme_snapshot,
            FocusTarget::ProcessMenuCopyJson,
            "JSON".to_string(),
            Message::CopyProcessJson,
            false,
        ),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    let close = focus::dynamic_button(
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
    .style(move |_| theme::panel_style(theme_snapshot))
    .width(Length::Fill)
    .into()
}

fn action_button<'a>(
    theme_snapshot: &'a Theme,
    target: FocusTarget,
    label: &'static str,
    action: ProcessMenuAction,
    destructive: bool,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    focus::dynamic_button(
        theme_snapshot,
        target,
        label.to_owned(),
        Message::ProcessMenuAction(action),
        destructive,
    )
}
