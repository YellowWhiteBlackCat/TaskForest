//! Bevy adapter for the application-correlated snapshot export session.

use std::sync::Arc;

use bevy::ecs::resource::Resource;
use taskforest_core::core::process::ProcessItem;
use taskmanager_app_host::SnapshotExportClient;
use taskmanager_application::i18n::t;
use taskmanager_application::session_slot::SessionSlot;
use taskmanager_application::snapshot_export::{
    SnapshotExportPayload, SnapshotExportSession, SnapshotExportState, SnapshotExportSubmitError,
    SnapshotExportTarget,
};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

/// The one installed snapshot-export session, held as a Bevy resource.
#[derive(Resource, Default)]
pub struct BevySnapshotExportRuntime(pub SessionSlot<SnapshotExportSession<SnapshotExportClient>>);

/// Submit one snapshot export from the single installed session.
pub fn request_snapshot_export(
    slot: &mut SessionSlot<SnapshotExportSession<SnapshotExportClient>>,
    shell: &mut ShellApp,
) {
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
    let Some(session) = slot.active_mut() else {
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

/// Drain one completed export into the shared notice line.
pub fn drain_completions(
    slot: &mut SessionSlot<SnapshotExportSession<SnapshotExportClient>>,
    shell: &mut ShellApp,
) -> bool {
    let Some(session) = slot.active_mut() else {
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
