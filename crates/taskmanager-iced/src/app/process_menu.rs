//! Renderer-local process context-menu vocabulary for the Iced adapter.
//!
//! The menu shape belongs to Iced, while every action is translated back to a
//! shared application/shell operation in `IcedApp::update`. Keeping this enum
//! toolkit-local avoids importing GPUI menu entities into the Iced graph.

use taskmanager_application::ProcessSignal;

use iced::Task;
use taskmanager_application::{
    AppAction, FrozenProcessIdentity, PlatformEffect, ProcessBatchAction, ResourceRevealRequest,
    UrlOpenRequest,
};
use taskmanager_shell::presentation::search_url_for;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use super::{IcedApp, Message};

/// One action exposed by the Applications-row context menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessMenuAction {
    EndTask,
    EndProcessTree,
    Kill,
    Suspend,
    Resume,
    Signal(ProcessSignal),
    OpenLocation,
    SearchOnline,
    Properties,
    CopyName,
    CopyPid,
    CopyCommandLine,
}

impl IcedApp {
    /// Apply one renderer-local menu choice after re-resolving the selected
    /// pid against the current process snapshot. The re-resolution is the
    /// Iced equivalent of GPUI's `menu_pid` + frozen identity path: a refresh
    /// cannot silently retarget an action to a different process.
    pub(super) fn apply_process_menu_action(
        &mut self,
        action: ProcessMenuAction,
        clipboard_task: &mut Option<Task<Message>>,
    ) -> Option<PlatformEffect> {
        let pid = self.process_menu_pid()?;
        self.close_context_menus();
        let Some(index) = self.shell.visible_process_index_of_pid(pid) else {
            self.shell.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                taskmanager_application::i18n::t("feedback.process_gone"),
            );
            return None;
        };
        let _ = self.shell.select_row(index);

        match action {
            ProcessMenuAction::EndTask => self.shell.apply_action(AppAction::RequestEndTask),
            ProcessMenuAction::EndProcessTree => self.shell.request_process_tree_end(pid),
            ProcessMenuAction::Kill => self.shell.request_process_batch(ProcessBatchAction::Kill),
            ProcessMenuAction::Suspend => self
                .shell
                .request_process_batch(ProcessBatchAction::Suspend),
            ProcessMenuAction::Resume => {
                self.shell.request_process_batch(ProcessBatchAction::Resume)
            }
            ProcessMenuAction::Signal(signal) => self.shell.request_process_signal(signal),
            ProcessMenuAction::OpenLocation => self.process_location_effect(),
            ProcessMenuAction::SearchOnline => self.process_search_effect(),
            ProcessMenuAction::Properties => self.shell.apply_action(AppAction::OpenProperties),
            ProcessMenuAction::CopyName => {
                self.copy_process_field(
                    pid,
                    "menu.copy_name",
                    |process| process.name.clone(),
                    clipboard_task,
                );
                None
            }
            ProcessMenuAction::CopyPid => {
                self.copy_process_field(
                    pid,
                    "menu.copy_pid",
                    |process| process.pid.to_string(),
                    clipboard_task,
                );
                None
            }
            ProcessMenuAction::CopyCommandLine => {
                self.copy_process_field(
                    pid,
                    "menu.copy_command_line",
                    |process| process.cmdline.clone(),
                    clipboard_task,
                );
                None
            }
        }
    }

    fn copy_process_field(
        &mut self,
        pid: u32,
        label_key: &'static str,
        value: impl FnOnce(&taskmanager_application::ProcessItem) -> String,
        clipboard_task: &mut Option<Task<Message>>,
    ) {
        let Some(process) = self.shell.visible_process_by_pid(pid) else {
            self.shell.report_notice(
                FeedbackSource::Interaction,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                taskmanager_application::i18n::t("feedback.process_gone"),
            );
            return;
        };
        let payload = value(process);
        if payload.trim().is_empty() {
            self.shell.report_notice(
                FeedbackSource::Clipboard,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                taskmanager_application::i18n::t("hint.nothing_to_copy"),
            );
            return;
        }
        self.shell.report_notice(
            FeedbackSource::Clipboard,
            FeedbackSeverity::Success,
            FeedbackLifecycle::SHORT,
            format!(
                "{} · {}",
                taskmanager_application::i18n::t("hint.copied"),
                taskmanager_application::i18n::t(label_key),
            ),
        );
        *clipboard_task = Some(iced::clipboard::write(payload));
    }

    pub(super) fn process_location_effect(&mut self) -> Option<PlatformEffect> {
        match self.shell.visible_process_at(self.shell.selected) {
            Some(process) => match FrozenProcessIdentity::from_process(process) {
                Some(target) => Some(PlatformEffect::RevealResource(ResourceRevealRequest {
                    target,
                    cached_executable: process.current_exe_path().map(ToOwned::to_owned),
                })),
                None => {
                    self.shell.report_notice(
                        FeedbackSource::Interaction,
                        FeedbackSeverity::Warning,
                        FeedbackLifecycle::SHORT,
                        taskmanager_application::i18n::t("hint.location_unavailable"),
                    );
                    None
                }
            },
            None => {
                self.shell.report_notice(
                    FeedbackSource::Interaction,
                    FeedbackSeverity::Warning,
                    FeedbackLifecycle::SHORT,
                    taskmanager_application::i18n::t("empty.no_process_selected"),
                );
                None
            }
        }
    }

    pub(super) fn process_search_effect(&mut self) -> Option<PlatformEffect> {
        match self.shell.visible_process_at(self.shell.selected) {
            Some(process) if !process.name.trim().is_empty() => {
                Some(PlatformEffect::OpenUrl(UrlOpenRequest {
                    url: search_url_for(&process.name),
                }))
            }
            _ => {
                self.shell.report_notice(
                    FeedbackSource::Interaction,
                    FeedbackSeverity::Warning,
                    FeedbackLifecycle::SHORT,
                    taskmanager_application::i18n::t("hint.no_process_name"),
                );
                None
            }
        }
    }
}
