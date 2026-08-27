//! GPUI adapter for the application-correlated snapshot export session.

use std::sync::Arc;

use taskmanager_app_host::SnapshotExportClient;
use taskmanager_application::snapshot_export::{
    SnapshotExportPayload, SnapshotExportSession, SnapshotExportState, SnapshotExportSubmitError,
    SnapshotExportTarget,
};
use taskmanager_application::{ProcessItem, SystemSnapshot};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};
use tracing::{info, warn};

use super::RootView;

#[derive(Debug, Default)]
pub(crate) enum SnapshotExportRuntime {
    #[default]
    Unavailable,
    Active(SnapshotExportSession<SnapshotExportClient>),
}

impl SnapshotExportRuntime {
    pub(crate) fn install(&mut self, client: SnapshotExportClient) {
        *self = Self::Active(SnapshotExportSession::new(client));
    }

    fn active_mut(&mut self) -> Option<&mut SnapshotExportSession<SnapshotExportClient>> {
        match self {
            Self::Unavailable => None,
            Self::Active(session) => Some(session),
        }
    }
}

impl RootView {
    pub(crate) fn request_snapshot_export(
        &mut self,
        snapshot: SystemSnapshot,
        processes: &[ProcessItem],
    ) {
        let Some(session) = self.snapshot_export.active_mut() else {
            self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                crate::i18n::t("system.export_unavailable"),
            );
            return;
        };
        let payload = SnapshotExportPayload::new(
            snapshot,
            Arc::<[ProcessItem]>::from(processes),
            SnapshotExportTarget::current_directory("taskmanager-snapshot"),
        );
        match session.submit(payload) {
            Ok(request) => {
                info!(
                    target: "taskmanager.export",
                    request = request.get(),
                    "snapshot export queued"
                );
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Info,
                    FeedbackLifecycle::UntilReplaced,
                    crate::i18n::t("system.export_queued"),
                );
            }
            Err(SnapshotExportSubmitError::Busy(_)) => self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Warning,
                FeedbackLifecycle::SHORT,
                crate::i18n::t("system.export_busy"),
            ),
            Err(SnapshotExportSubmitError::RequestSpaceExhausted) => self.shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                crate::i18n::t("system.export_unavailable"),
            ),
            Err(SnapshotExportSubmitError::Rejected(error)) => {
                warn!(
                    target: "taskmanager.export",
                    kind = error.kind().code(),
                    detail = error.detail(),
                    "snapshot export rejected"
                );
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    crate::i18n::t("system.export_failed").replace("{}", error.detail()),
                );
            }
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
            SnapshotExportState::Ready { request, base } => {
                info!(
                    target: "taskmanager.export",
                    request = request.get(),
                    base = base.as_ref(),
                    "snapshot export completed"
                );
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    crate::i18n::t("system.export_success").replace("{}", &base),
                );
            }
            SnapshotExportState::Failed { request, error } => {
                warn!(
                    target: "taskmanager.export",
                    request = request.get(),
                    kind = error.kind().code(),
                    detail = error.detail(),
                    "snapshot export failed"
                );
                self.shell.report_notice(
                    FeedbackSource::Persistence,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    crate::i18n::t("system.export_failed").replace("{}", error.detail()),
                );
            }
            SnapshotExportState::Closed
            | SnapshotExportState::Queued(_)
            | SnapshotExportState::Running(_) => return false,
        }
        true
    }
}
