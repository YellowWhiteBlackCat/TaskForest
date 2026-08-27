//! Window-local systems that materialize the shared platform fold.

use gpui::Context;
use taskmanager_application::{DesktopAppearanceEvent, RefreshRequest, SystemTelemetryDomain};
use taskmanager_shell::BatchFoldOutput;

use super::super::ProcessControlAction;
use super::{
    PlatformBatchChanges, RootView, ingest_correlated_system_outcome,
    record_history_ingestion_error,
};

impl RootView {
    pub(super) fn sync_telemetry_system(
        &mut self,
        output: &BatchFoldOutput,
        changes: &mut PlatformBatchChanges,
    ) {
        if !output.changes.telemetry {
            return;
        }
        if output.changes.frame_commit.is_committed()
            && let Some(snapshot) = self.projection().snapshot.clone()
        {
            self.materialized
                .replace_snapshot(self.projection().system_revision, snapshot);
        }
        changes.telemetry = true;
        changes.frame_commit = output.changes.frame_commit;
        for correlated in &output.system_telemetry_outcomes {
            match ingest_correlated_system_outcome(&self.telemetry_ingestor, correlated) {
                Ok(_) if correlated.event.domain() == SystemTelemetryDomain::Cpu => {
                    self.cpu_core_history.bump();
                }
                Ok(_) if correlated.event.domain() == SystemTelemetryDomain::Memory => {
                    self.memory_history.bump();
                }
                Err(error) => record_history_ingestion_error(
                    &mut self.system_history_ingestion_diagnostics,
                    correlated,
                    error,
                ),
                Ok(_) => {}
            }
        }
    }

    pub(super) fn sync_hardware_system(&mut self, output: &BatchFoldOutput) {
        if output.changes.hardware {
            self.materialized.replace_hardware(
                self.projection().system_revision,
                self.projection().hardware.clone().unwrap_or_default(),
                self.projection()
                    .hardware_source
                    .clone()
                    .unwrap_or_default(),
            );
            self.submit_npu_inventory_refresh();
        }
        if output.changes.containers {
            self.materialized.replace_containers(
                self.projection().system_revision,
                self.projection().containers.clone().unwrap_or_default(),
            );
        }
    }

    pub(super) fn sync_process_inventory_system(
        &mut self,
        output: &BatchFoldOutput,
        changes: &mut PlatformBatchChanges,
    ) {
        if !output.changes.processes {
            return;
        }
        self.materialized.replace_processes(
            self.projection().process_revision,
            self.projection().processes.clone().unwrap_or_default(),
        );
        let live_pids = self.processes().iter().map(|process| process.pid).collect();
        self.shell.selection.retain_live(&live_pids);
        if let Some(pid) = self.process_affinity_pid() {
            let expected = match self.shell.process_affinity_state() {
                taskmanager_application::ProcessAffinityState::Loading { target, .. }
                | taskmanager_application::ProcessAffinityState::Failed { target, .. } => {
                    Some(target.clone())
                }
                taskmanager_application::ProcessAffinityState::Ready(ready) => {
                    Some(ready.target.clone())
                }
                taskmanager_application::ProcessAffinityState::Closed => None,
            };
            if expected.is_some_and(|target| self.frozen_process(pid).as_ref() != Some(&target)) {
                self.dismiss_window_surface(
                    super::super::WindowSurfaceKind::ProcessAffinity,
                    super::super::WindowSurfaceDismissReason::TargetUnavailable,
                );
            }
        }
        changes.processes = true;
    }

    pub(super) fn sync_service_inventory_system(
        &mut self,
        output: &BatchFoldOutput,
        changes: &mut PlatformBatchChanges,
    ) {
        if !output.changes.services {
            return;
        }
        self.materialized.replace_services(
            self.projection().services_revision,
            self.projection().services.clone().unwrap_or_default(),
            self.projection()
                .services_source
                .clone()
                .unwrap_or_default(),
        );
        if self.selected_service.as_ref().is_some_and(|id| {
            id.as_str().is_empty() || !self.services().iter().any(|service| &service.id == id)
        }) {
            self.selected_service = None;
        }
        if self.service_details_target().is_some_and(|id| {
            id.as_str().is_empty() || !self.services().iter().any(|service| &service.id == id)
        }) {
            self.dismiss_window_surface(
                super::super::WindowSurfaceKind::ServiceDetails,
                super::super::WindowSurfaceDismissReason::TargetUnavailable,
            );
        }
        changes.services = true;
    }

    pub(super) fn sync_startup_inventory_system(
        &mut self,
        output: &BatchFoldOutput,
        changes: &mut PlatformBatchChanges,
    ) {
        if output.changes.startup {
            self.materialized.replace_startup(
                self.projection().startup_revision,
                self.projection()
                    .startup_entries
                    .clone()
                    .unwrap_or_default(),
                self.projection().startup_source.clone().unwrap_or_default(),
            );
            if self
                .selected_startup
                .as_ref()
                .is_some_and(|id| !self.startup_entries().iter().any(|entry| &entry.id == id))
            {
                self.selected_startup = None;
            }
            changes.startup = true;
        }
        if output.changes.startup_evidence {
            self.materialized.replace_startup_evidence(
                self.projection().refresh_count,
                self.projection().startup_boot_evidence.clone(),
                self.projection().startup_evidence_unavailable,
            );
            changes.startup_evidence = true;
        }
    }

    pub(super) fn sync_session_inventory_system(&mut self, output: &BatchFoldOutput) {
        if !output.changes.sessions {
            return;
        }
        self.materialized.replace_sessions(
            self.projection().sessions_revision,
            self.projection().sessions.clone().unwrap_or_default(),
            self.projection()
                .sessions_source
                .clone()
                .unwrap_or_default(),
        );
        if self
            .selected_session
            .as_ref()
            .is_some_and(|id| !self.sessions().iter().any(|session| &session.id == id))
        {
            self.selected_session = None;
        }
    }

    pub(super) fn sync_control_outcome_system(
        &mut self,
        output: &BatchFoldOutput,
        cx: &mut Context<Self>,
    ) {
        if let Some(feedback) = output.process_feedback.as_ref() {
            self.accept_shared_process_control_feedback(feedback);
            self.request_refresh(RefreshRequest::Processes);
        }
        for (_, result) in &output.batch_results {
            self.accept_process_batch_result(result.clone(), cx);
        }
        self.sync_affinity_outcome(output, cx);
        if output.changes.process_insights
            && let Some(projection) = self.projection().process_insights.clone()
        {
            self.apply_process_insights_projection(projection);
        }
        for outcome in &output.service_control_outcomes {
            self.apply_service_control_outcome_from_shared(outcome.clone());
        }
        let service_updates = output
            .service_log_updates
            .iter()
            .chain(&output.service_updates)
            .cloned()
            .collect();
        self.apply_service_updates(service_updates);
        for outcome in &output.startup_control_outcomes {
            self.apply_startup_outcome_from_shared(outcome.clone());
        }
        for outcome in &output.session_control_outcomes {
            self.apply_session_outcome_from_shared(outcome.clone(), cx);
        }
    }

    fn sync_affinity_outcome(&mut self, output: &BatchFoldOutput, cx: &mut Context<Self>) {
        if !output.changes.process_affinity {
            return;
        }
        match self.shell.process_affinity_state().clone() {
            taskmanager_application::ProcessAffinityState::Ready(ready) => {
                self.processes_state.affinity_editor.cpus = ready.cpus.into_iter().collect();
            }
            taskmanager_application::ProcessAffinityState::Failed {
                target, failure, ..
            } => {
                self.processes_state.affinity_editor.cpus.clear();
                self.record_process_control_result(
                    ProcessControlAction::SetAffinity,
                    target.pid,
                    Err(failure),
                    cx,
                );
            }
            taskmanager_application::ProcessAffinityState::Closed
            | taskmanager_application::ProcessAffinityState::Loading { .. } => {}
        }
    }

    pub(super) fn sync_dynamic_device_system(
        &mut self,
        output: &BatchFoldOutput,
        changes: &mut PlatformBatchChanges,
    ) {
        if output.changes.dynamic_devices {
            if let Some((sensors, sources, stamp)) = self.projection().sensor_projection()
                && self.materialized.replace_sensors(
                    stamp.sequence,
                    sensors.clone(),
                    sources.to_vec(),
                )
                && let Some(stamp) =
                    taskmanager_telemetry_store::CorrelatedTelemetryStamp::from_accepted_event(
                        stamp.sequence,
                        stamp.observed_at_ms,
                    )
            {
                let _ = self
                    .telemetry_ingestor
                    .ingest_correlated_sensors(stamp, self.sensors());
            }
            if let Some((power_supplies, sources, stamp)) =
                self.projection().power_supply_projection()
                && self.materialized.replace_power_supplies(
                    stamp.sequence,
                    power_supplies.clone(),
                    sources.to_vec(),
                )
                && let Some(stamp) =
                    taskmanager_telemetry_store::CorrelatedTelemetryStamp::from_accepted_event(
                        stamp.sequence,
                        stamp.observed_at_ms,
                    )
            {
                let _ = self
                    .telemetry_ingestor
                    .ingest_correlated_power_supplies(stamp, self.power_supplies());
            }
            changes.dynamic_devices = true;
        }
    }

    pub(super) fn apply_frontend_event_system(
        &mut self,
        output: &BatchFoldOutput,
        cx: &mut Context<Self>,
    ) {
        for event in &output.desktop_appearance_events {
            let DesktopAppearanceEvent::Snapshot(snapshot) = &event.event;
            self.desktop_appearance = snapshot.value;
            self.desktop_appearance_sources
                .clone_from(&snapshot.sources);
            self.apply_desktop_appearance(cx);
        }
        if output.changes.storage_health
            && let Some((filesystems, sources)) = self.projection().storage_health_projection()
        {
            self.materialized.replace_storage_health(
                self.projection().refresh_count,
                filesystems.clone(),
                sources.to_vec(),
            );
        }
        if output.changes.directory_usage {
            self.materialized.replace_directory_usage(
                self.projection().refresh_count,
                self.projection().directory_usage.clone(),
            );
        }
        if output.changes.npu_inventory {
            self.materialized.replace_npu_inventory(
                self.projection().refresh_count,
                self.projection().npu_inventory.clone(),
            );
        }
        for event in &output.shell_events {
            self.apply_shell_event(event.clone(), cx);
        }
        for event in &output.setup_script_events {
            self.apply_first_run_event(event.clone(), cx);
        }
    }

    pub(super) fn sync_alert_system(&mut self, output: &BatchFoldOutput) {
        if !output.changes.snapshot_recorded {
            return;
        }
        let previous = self.active_alerts().to_vec();
        let next = self.projection().alert_active.clone();
        let timestamp_ms = self.system_snapshot().timestamp_ms;
        self.dashboard
            .events
            .observe(&previous, &next, timestamp_ms);
        self.materialized
            .replace_active_alerts(self.projection().refresh_count, next);
        let notifications = self.shell.drain_alert_notifications();
        self.submit_alert_notifications(notifications);
    }

    pub(super) fn apply_failure_system(
        &mut self,
        output: &BatchFoldOutput,
        cx: &mut Context<Self>,
    ) {
        for failure in &output.failures {
            self.apply_shell_failure(failure, cx);
            self.apply_first_run_failure(failure, cx);
        }
        self.record_platform_failures(output.failures.clone());
    }
}
