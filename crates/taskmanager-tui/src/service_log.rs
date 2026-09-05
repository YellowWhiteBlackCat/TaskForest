//! TUI service-log export adapter.

use std::path::PathBuf;

use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use crate::TuiApp;

impl TuiApp {
    /// Export the currently visible service-log entries to `taskmanager-service-{id}.log`.
    pub fn export_service_log(&mut self) {
        let Some(open) = self.shell.service_log.as_ref() else {
            self.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                taskmanager_application::i18n::t("svc.logs_nothing_to_export"),
            );
            return;
        };
        let Some(service_id) = open.service_id().map(ToString::to_string) else {
            self.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                taskmanager_application::i18n::t("svc.logs_nothing_to_export"),
            );
            return;
        };
        let entries = self
            .shell
            .visible_service_log_entries(self.service_log_now_micros)
            .unwrap_or_default()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        if entries.is_empty() {
            self.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                taskmanager_application::i18n::t("svc.logs_nothing_to_export"),
            );
            return;
        }

        let safe_id: String = service_id
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                    character
                } else {
                    '-'
                }
            })
            .collect();
        let file_name = format!("taskmanager-service-{safe_id}.log");
        let destination = self
            .export_dir
            .as_ref()
            .map_or_else(|| PathBuf::from(&file_name), |dir| dir.join(&file_name));

        let mut payload = entries
            .iter()
            .map(|entry| format!("[{:?}] {}", entry.level, entry.message))
            .collect::<Vec<_>>()
            .join("\n");
        if !payload.is_empty() {
            payload.push('\n');
        }

        match std::fs::write(&destination, payload) {
            Ok(()) => {
                self.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    taskmanager_application::i18n::t("svc.logs_exported")
                        .replace("{path}", &destination.display().to_string()),
                );
            }
            Err(_) => {
                self.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    taskmanager_application::i18n::t("svc.logs_export_failed"),
                );
            }
        }
    }
}
