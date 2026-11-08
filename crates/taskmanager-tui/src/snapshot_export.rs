//! TUI adapter for the application-correlated snapshot export session.

use std::sync::Arc;

use taskforest_core::core::process::ProcessItem;
use taskmanager_app_host::SnapshotExportClient;
use taskmanager_application::session_slot::SessionSlot;
use taskmanager_application::snapshot_export::{
    SnapshotExportPayload, SnapshotExportSession, SnapshotExportState, SnapshotExportSubmitError,
    SnapshotExportTarget,
};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use crate::TuiApp;
use taskmanager_application::i18n::t;

pub(crate) type TuiSnapshotExportRuntime = SessionSlot<SnapshotExportSession<SnapshotExportClient>>;

impl TuiApp {
    pub(crate) fn install_snapshot_export_client(&mut self, client: SnapshotExportClient) {
        self.snapshot_export
            .install_with(|| SnapshotExportSession::new(client));
    }

    pub fn export_snapshot(&mut self) {
        let Some(snapshot) = self.projection().snapshot.clone() else {
            self.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                t("system.export_no_data"),
            );
            return;
        };
        let processes = self
            .projection()
            .processes
            .as_ref()
            .map_or_else(Vec::new, |processes| processes.as_slice().to_vec());
        let stem = format!("taskmanager-snapshot-{}", snapshot.timestamp_ms);
        let target = self.export_dir.as_ref().map_or_else(
            || SnapshotExportTarget::current_directory(stem.clone()),
            |directory| SnapshotExportTarget::base_path(directory.join(&stem)),
        );
        let payload =
            SnapshotExportPayload::new(snapshot, Arc::<[ProcessItem]>::from(processes), target);
        let Some(session) = self.snapshot_export.active_mut() else {
            self.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                t("system.export_unavailable"),
            );
            return;
        };
        match session.submit(payload) {
            Ok(_) => self.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Info,
                FeedbackLifecycle::UntilReplaced,
                t("system.export_queued"),
            ),
            Err(SnapshotExportSubmitError::Busy(_)) => self.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                t("system.export_busy"),
            ),
            Err(SnapshotExportSubmitError::RequestSpaceExhausted) => self.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                t("system.export_unavailable"),
            ),
            Err(SnapshotExportSubmitError::Rejected(error)) => self.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                t("system.export_failed").replace("{}", error.detail()),
            ),
        }
    }

    pub(crate) fn drain_snapshot_export_completions(&mut self) -> bool {
        let Some(session) = self.snapshot_export.active_mut() else {
            return false;
        };
        if session.drain() == 0 {
            return false;
        }
        let state = session.state().clone();
        match state {
            SnapshotExportState::Ready { base, .. } => self.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Success,
                FeedbackLifecycle::SHORT,
                t("system.snapshot_exported_to").replace("{}", &base),
            ),
            SnapshotExportState::Failed { error, .. } => self.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                t("system.export_failed").replace("{}", error.detail()),
            ),
            SnapshotExportState::Closed
            | SnapshotExportState::Queued(_)
            | SnapshotExportState::Running(_) => return false,
        }
        true
    }
}
