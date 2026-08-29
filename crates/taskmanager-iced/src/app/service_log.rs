//! Iced-owned service-log export adapter.
//!
//! The feed and filtering stay in `taskmanager-shell`; this module only turns
//! the currently visible, already-filtered entries into the shared bounded
//! diagnostic worker used for file publication.

use iced::Task;
use taskmanager_app_host::DiagnosticBundleClient;
use taskmanager_application::PlatformEffect;
use taskmanager_application::{
    DiagnosticBundleSession, DiagnosticBundleTarget, prepare_service_log_bundle,
};

use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use super::{IcedApp, Message};

#[derive(Debug, Default)]
pub(super) enum IcedServiceLogExportRuntime {
    #[default]
    Unavailable,
    Active(DiagnosticBundleSession<DiagnosticBundleClient>),
}

impl IcedServiceLogExportRuntime {
    fn active_mut(&mut self) -> Option<&mut DiagnosticBundleSession<DiagnosticBundleClient>> {
        match self {
            Self::Unavailable => None,
            Self::Active(session) => Some(session),
        }
    }
}

impl IcedApp {
    pub(crate) fn install_service_log_export_client(&mut self, client: DiagnosticBundleClient) {
        self.service_log_export =
            IcedServiceLogExportRuntime::Active(DiagnosticBundleSession::new(client));
    }

    pub(super) fn open_service_log_effect(&mut self) -> Option<PlatformEffect> {
        self.close_context_menus();
        self.close_local_modals();
        self.close_shell_modals();
        self.shell.open_service_log()
    }

    pub(super) fn open_service_log_for_effect(&mut self, index: usize) -> Option<PlatformEffect> {
        self.close_context_menus();
        self.close_local_modals();
        self.close_shell_modals();
        let service_id = self
            .shell
            .projection()
            .services
            .as_deref()
            .and_then(|services| services.get(index))
            .map(|service| service.id.clone());
        service_id.and_then(|id| self.shell.open_service_log_for(id))
    }

    pub(super) fn close_service_log_effect(&mut self) -> Option<PlatformEffect> {
        self.shell.close_service_log();
        None
    }

    pub(super) fn toggle_service_log_follow_effect(&mut self) -> Option<PlatformEffect> {
        self.shell.toggle_service_log_follow();
        None
    }

    pub(super) fn toggle_service_log_paused_effect(&mut self) -> Option<PlatformEffect> {
        self.shell.toggle_service_log_paused();
        None
    }

    pub(super) fn cycle_service_log_level_effect(&mut self) -> Option<PlatformEffect> {
        self.shell.cycle_service_log_level();
        None
    }

    pub(super) fn cycle_service_log_time_effect(&mut self) -> Option<PlatformEffect> {
        self.shell.cycle_service_log_time();
        None
    }

    pub(super) fn copy_service_log(&mut self, clipboard_task: &mut Option<Task<Message>>) {
        let entries = self
            .shell
            .visible_service_log_entries(self.service_log_now_micros())
            .unwrap_or_default();
        if entries.is_empty() {
            self.shell.report_notice(
                FeedbackSource::Clipboard,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                taskmanager_application::i18n::t("svc.logs_nothing_to_copy"),
            );
        } else {
            let payload = entries
                .iter()
                .map(|entry| format!("[{:?}] {}", entry.level, entry.message))
                .collect::<Vec<_>>()
                .join("\n");
            self.shell.report_notice(
                FeedbackSource::Clipboard,
                FeedbackSeverity::Success,
                FeedbackLifecycle::SHORT,
                format!(
                    "{} · {}",
                    taskmanager_application::i18n::t("hint.copied"),
                    taskmanager_application::i18n::t("svc.logs"),
                ),
            );
            *clipboard_task = Some(iced::clipboard::write(payload));
        }
    }

    pub(super) fn export_service_log(&mut self) -> Option<PlatformEffect> {
        self.submit_service_log_export();
        None
    }

    pub(super) fn submit_service_log_export(&mut self) {
        let entries = self
            .shell
            .visible_service_log_entries(self.service_log_now_micros())
            .unwrap_or_default()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        if entries.is_empty() {
            self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                taskmanager_application::i18n::t("svc.logs_nothing_to_export"),
            );
            return;
        }
        let Some(service_id) = self
            .shell
            .service_log
            .as_ref()
            .and_then(|open| open.service_id().map(ToString::to_string))
        else {
            return;
        };
        let Ok(plan) = prepare_service_log_bundle(&entries) else {
            self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                taskmanager_application::i18n::t("svc.logs_export_failed"),
            );
            return;
        };
        let safe_name = service_id
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                    character
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let file_name = format!("taskmanager-{safe_name}-logs.json");
        let Some(session) = self.service_log_export.active_mut() else {
            self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                taskmanager_application::i18n::t("svc.logs_export_failed"),
            );
            return;
        };
        match session.submit(plan, DiagnosticBundleTarget::current_directory(file_name)) {
            Ok(_) => {
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::UntilReplaced,
                    taskmanager_application::i18n::t("svc.logs_exporting"),
                );
            }
            Err(_) => {
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    taskmanager_application::i18n::t("svc.logs_export_failed"),
                );
            }
        }
    }

    pub(super) fn poll_service_log_export(&mut self) {
        let Some(result) = self
            .service_log_export
            .active_mut()
            .and_then(|session| session.drain().into_iter().next())
        else {
            return;
        };
        let (severity, lifecycle, text) = match result.result {
            Ok(()) => (
                FeedbackSeverity::Success,
                FeedbackLifecycle::SHORT,
                taskmanager_application::i18n::t("svc.logs_exported")
                    .replace("{path}", &result.destination.display().to_string()),
            ),
            Err(_) => (
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                taskmanager_application::i18n::t("svc.logs_export_failed").to_owned(),
            ),
        };
        self.shell
            .report_notice(FeedbackSource::Persistence, severity, lifecycle, text);
    }
}
