//! Direct consumption of correlated application events into RootView state.

use gpui::Context;
use taskmanager_application::PlatformEventBatch;
use taskmanager_shell::{BatchFoldOutput, FrameCommit};

use super::RootView;
use super::system_telemetry::{ingest_correlated_system_outcome, record_history_ingestion_error};

mod systems;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PlatformBatchChanges {
    pub(super) telemetry: bool,
    /// A complete immutable telemetry frame was published by the shared
    /// projection. Partial domain arrivals intentionally leave this
    /// transition as `Unchanged`.
    pub(super) frame_commit: FrameCommit,
    pub(super) dynamic_devices: bool,
    pub(super) processes: bool,
    pub(super) services: bool,
    pub(super) startup: bool,
    pub(super) startup_evidence: bool,
}

impl RootView {
    /// Fold one platform batch through the shared renderer-neutral projection
    /// store, then materialize this window's revision-keyed read models from
    /// the typed change report. The shared store is the single data-truth
    /// fold. Remaining shell/setup/appearance side outputs trigger UI effects;
    /// they are never interpreted as a second data projection.
    pub(crate) fn apply_platform_event_batch(
        &mut self,
        batch: PlatformEventBatch,
        cx: &mut Context<Self>,
    ) -> PlatformBatchChanges {
        let output = self.shell.apply_platform_batch(batch);
        self.sync_view_from_shared_projection(output, cx)
    }

    fn sync_view_from_shared_projection(
        &mut self,
        output: BatchFoldOutput,
        cx: &mut Context<Self>,
    ) -> PlatformBatchChanges {
        let mut changes = PlatformBatchChanges::default();
        self.sync_telemetry_system(&output, &mut changes);
        self.sync_hardware_system(&output);
        self.sync_process_inventory_system(&output, &mut changes);
        self.sync_service_inventory_system(&output, &mut changes);
        self.sync_startup_inventory_system(&output, &mut changes);
        self.sync_session_inventory_system(&output);
        self.sync_control_outcome_system(&output, cx);
        self.sync_dynamic_device_system(&output, &mut changes);
        self.apply_frontend_event_system(&output, cx);
        self.sync_alert_system(&output);
        self.apply_failure_system(&output, cx);
        changes
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_platform_batch_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_platform_batch_pipeline_tests.rs"]
mod pipeline_tests;
