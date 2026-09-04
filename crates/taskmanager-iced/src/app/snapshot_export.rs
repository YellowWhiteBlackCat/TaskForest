//! Iced adapter for the shared snapshot-export lifecycle and app-host port.

use std::sync::Arc;

use taskmanager_app_host::SnapshotExportClient;
use taskmanager_application::snapshot_export::{
    SnapshotExportPayload, SnapshotExportSession, SnapshotExportState, SnapshotExportSubmitError,
    SnapshotExportTarget,
};
use taskmanager_core::core::process::ProcessItem;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use super::IcedApp;

#[derive(Debug, Default)]
pub(super) enum IcedSnapshotExportRuntime {
    #[default]
    Unavailable,
    Active(SnapshotExportSession<SnapshotExportClient>),
}

impl IcedSnapshotExportRuntime {
    pub(super) fn install(&mut self, client: SnapshotExportClient) {
        *self = Self::Active(SnapshotExportSession::new(client));
    }

    fn active_mut(&mut self) -> Option<&mut SnapshotExportSession<SnapshotExportClient>> {
        match self {
            Self::Unavailable => None,
            Self::Active(session) => Some(session),
        }
    }
}

impl IcedApp {
    pub(crate) fn install_snapshot_export_client(&mut self, client: SnapshotExportClient) {
        self.snapshot_export.install(client);
    }

    pub(super) fn request_snapshot_export(&mut self) {
        let (Some(snapshot), Some(processes)) = (
            self.shell.projection().snapshot.clone(),
            self.shell.projection().processes.as_ref(),
        ) else {
            self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::TIMED_SHORT,
                taskmanager_application::i18n::t("system.export_no_data"),
            );
            return;
        };
        let stem = format!("taskmanager-snapshot-{}", snapshot.timestamp_ms);
        let payload = SnapshotExportPayload::new(
            snapshot,
            Arc::<[ProcessItem]>::from(processes.as_slice()),
            SnapshotExportTarget::current_directory(stem),
        );
        let Some(session) = self.snapshot_export.active_mut() else {
            self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::TIMED_LONG,
                taskmanager_application::i18n::t("system.export_unavailable"),
            );
            return;
        };
        match session.submit(payload) {
            Ok(_) => self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Info,
                FeedbackLifecycle::TIMED_SHORT,
                taskmanager_application::i18n::t("system.export_queued"),
            ),
            Err(SnapshotExportSubmitError::Busy(_)) => self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::TIMED_SHORT,
                taskmanager_application::i18n::t("system.export_busy"),
            ),
            Err(SnapshotExportSubmitError::RequestSpaceExhausted) => self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::TIMED_LONG,
                taskmanager_application::i18n::t("system.export_unavailable"),
            ),
            Err(SnapshotExportSubmitError::Rejected(error)) => self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::TIMED_LONG,
                taskmanager_application::i18n::t("system.export_failed")
                    .replace("{}", error.detail()),
            ),
        }
    }

    pub(super) fn drain_snapshot_export_completions(&mut self) -> bool {
        let Some(session) = self.snapshot_export.active_mut() else {
            return false;
        };
        if session.drain() == 0 {
            return false;
        }
        let state = session.state().clone();
        match state {
            SnapshotExportState::Ready { base, .. } => self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Success,
                FeedbackLifecycle::TIMED_SHORT,
                taskmanager_application::i18n::t("system.export_success").replace("{}", &base),
            ),
            SnapshotExportState::Failed { error, .. } => self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::TIMED_LONG,
                taskmanager_application::i18n::t("system.export_failed")
                    .replace("{}", error.detail()),
            ),
            SnapshotExportState::Closed
            | SnapshotExportState::Queued(_)
            | SnapshotExportState::Running(_) => return false,
        }
        true
    }
}
