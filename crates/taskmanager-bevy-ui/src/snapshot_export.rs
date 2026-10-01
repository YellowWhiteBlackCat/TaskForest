//! Bevy adapter for the application-correlated snapshot export session.

use std::sync::Arc;

use taskmanager_app_host::SnapshotExportClient;
use taskmanager_application::i18n::t;
use taskmanager_application::snapshot_export::{
    SnapshotExportPayload, SnapshotExportSession, SnapshotExportState, SnapshotExportSubmitError,
    SnapshotExportTarget,
};
use taskmanager_core::core::process::ProcessItem;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

/// Bevy snapshot export runtime holder.
#[derive(Debug, Default)]
pub enum BevySnapshotExportRuntime {
    #[default]
    Unavailable,
    Active(SnapshotExportSession<SnapshotExportClient>),
}

impl BevySnapshotExportRuntime {
    pub fn install(&mut self, client: SnapshotExportClient) {
        *self = Self::Active(SnapshotExportSession::new(client));
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Active(_))
    }

    fn active_mut(&mut self) -> Option<&mut SnapshotExportSession<SnapshotExportClient>> {
        match self {
            Self::Unavailable => None,
            Self::Active(session) => Some(session),
        }
    }

    pub fn request_snapshot_export(&mut self, shell: &mut ShellApp) {
        let (Some(snapshot), Some(processes)) = (
            shell.projection().snapshot.clone(),
            shell.projection().processes.as_ref(),
        ) else {
            shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::TIMED_SHORT,
                t("system.export_no_data"),
            );
            return;
        };
        let stem = format!("taskmanager-snapshot-{}", snapshot.timestamp_ms);
        let payload = SnapshotExportPayload::new(
            snapshot,
            Arc::<[ProcessItem]>::from(processes.as_slice()),
            SnapshotExportTarget::current_directory(stem),
        );
        let Some(session) = self.active_mut() else {
            shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::TIMED_LONG,
                t("system.export_unavailable"),
            );
            return;
        };
        match session.submit(payload) {
            Ok(_) => shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Info,
                FeedbackLifecycle::TIMED_SHORT,
                t("system.export_queued"),
            ),
            Err(SnapshotExportSubmitError::Busy(_)) => shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::TIMED_SHORT,
                t("system.export_busy"),
            ),
            Err(SnapshotExportSubmitError::RequestSpaceExhausted) => shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::TIMED_LONG,
                t("system.export_unavailable"),
            ),
            Err(SnapshotExportSubmitError::Rejected(error)) => shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::TIMED_LONG,
                t("system.export_failed").replace("{}", error.detail()),
            ),
        }
    }

    pub fn drain_completions(&mut self, shell: &mut ShellApp) -> bool {
        let Some(session) = self.active_mut() else {
            return false;
        };
        if session.drain() == 0 {
            return false;
        }
        let state = session.state().clone();
        match state {
            SnapshotExportState::Ready { base, .. } => {
                shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::TIMED_SHORT,
                    t("system.export_success").replace("{}", &base),
                );
                true
            }
            SnapshotExportState::Failed { error, .. } => {
                shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::TIMED_LONG,
                    t("system.export_failed").replace("{}", error.detail()),
                );
                true
            }
            SnapshotExportState::Closed
            | SnapshotExportState::Queued(_)
            | SnapshotExportState::Running(_) => false,
        }
    }
}
