//! Capture-only systems applied after one accepted platform batch.

use gpui::Context;
use taskmanager_application::{PendingConfirmation, PlatformEventBatch};
use taskmanager_core::core::process::ProcessLiveKey;

use super::super::{
    CaptureEvidence, CaptureProcessAction, DiagnosticBundleUiState, ProcessDetailsSection,
    RootView, SelectedDevice, TopPage, WindowSurfaceKind,
};
use crate::gpui_app::dashboard::SystemSection;
use crate::gpui_app::first_run::FirstRunPhase;
use taskmanager_application::process_category_projection::category_expansion_key;

pub(super) fn apply_platform_batch(
    view: &mut RootView,
    batch: PlatformEventBatch,
    cx: &mut Context<RootView>,
) {
    let changes = view.apply_platform_event_batch(batch, cx);
    if changes.frame_commit.is_committed() {
        view.sync_capture_snapshot_system();
        view.telemetry_frame_state = view.projection().telemetry_frame_state();
    }
    if changes.frame_commit.is_committed() || changes.dynamic_devices {
        let selected = view.selected;
        if view.capture_evidence.preserve_hotplug_selection() {
            view.reconcile_device_selection();
        } else {
            view.select_device(selected);
        }
    }
    apply_process_capture(view, changes.processes, cx);
    apply_inventory_capture(
        view,
        changes.services,
        changes.startup,
        changes.startup || changes.startup_evidence,
        cx,
    );
    apply_shell_capture(view, cx);
    view.publish_accessibility_snapshot();
    cx.notify();
}

fn apply_process_capture(view: &mut RootView, processes_updated: bool, cx: &mut Context<RootView>) {
    if let Some(action) = view.sync_capture_process_system(processes_updated) {
        match action {
            CaptureProcessAction::Termination(intent) => {
                view.arm_confirmation(PendingConfirmation::ProcessTermination(intent));
            }
            CaptureProcessAction::ApplicationSelection(root_identity) => {
                view.page = TopPage::Apps;
                view.select_application_root(root_identity);
            }
            CaptureProcessAction::Batch(intent) => {
                view.page = TopPage::Apps;
                let anchor = intent.targets.last().and_then(|target| target.live_key());
                view.replace_process_selection(
                    intent.targets.iter().filter_map(|target| target.live_key()),
                    anchor,
                );
                view.arm_confirmation(PendingConfirmation::ProcessBatch(intent));
                view.capture_evidence.mark_process_batch_ready(
                    view.process_batch_confirmation().is_some(),
                    view.selected_process_count(),
                );
            }
            CaptureProcessAction::Properties(identity, section) => {
                view.open_process_details(identity, section);
            }
            CaptureProcessAction::Insights { identity, state } => {
                view.open_process_details(identity, ProcessDetailsSection::Insights);
                if let Some(target) = view.frozen_process(identity) {
                    view.process_insights.install_capture_state(target, state);
                }
                let ready = view.process_properties_identity() == Some(identity)
                    && view.details_section == ProcessDetailsSection::Insights
                    && view
                        .processes()
                        .iter()
                        .any(|process| ProcessLiveKey::from_process(process) == Some(identity))
                    && view.process_insights.is_ready_for(identity);
                view.capture_evidence.mark_process_insights_ready(ready);
            }
        }
    }
    apply_process_page_capture(view, cx);
}

fn apply_process_page_capture(view: &mut RootView, cx: &mut Context<RootView>) {
    if view.capture_evidence.apps_search_highlight_requested() {
        view.page = TopPage::Apps;
        view.set_process_query("f");
        if let Some(input) = view.search_input.as_ref().cloned() {
            input.update(cx, |state, cx| state.set_value("f", cx));
        }
    }
    if view.capture_evidence.sidebar_hidden_requested() {
        view.page = TopPage::Performance;
        view.sidebar_visible = false;
        view.capture_evidence
            .mark_sidebar_hidden_ready(!view.sidebar_visible);
    }
    if view.capture_evidence.sidebar_edit_requested() {
        view.page = TopPage::Performance;
        view.sidebar_visible = true;
        view.sidebar_edit_mode = true;
        view.capture_evidence
            .mark_sidebar_edit_ready(view.sidebar_edit_mode);
    }
    if view.capture_evidence.telemetry_paused_requested() {
        view.page = TopPage::Performance;
        view.telemetry_refresh_policy
            .apply(taskmanager_application::TelemetryRefreshPolicyChange::SetControlHeld(true));
        view.capture_evidence
            .mark_telemetry_paused_ready(view.telemetry_refresh_policy.is_paused());
    }
    if view.capture_evidence.apps_group_expanded_requested() {
        configure_category_apps(view);
        let (rows, _, _) = view.processes_projection();
        let expanded = rows
            .iter()
            .any(|row| row.depth == 0 && row.has_children && !row.collapsed)
            && rows.iter().any(|row| row.depth >= 1);
        view.capture_evidence
            .mark_apps_group_expanded_ready(expanded);
    }
    if view.capture_evidence.apps_identity_matrix_requested() {
        view.set_process_query("");
        configure_category_apps(view);
        let (rows, _, _) = view.processes_projection();
        let expected = ["Mail PWA", "Firefox (Snap)", "Portable Editor (AppImage)"];
        let ready = expected.iter().all(|name| {
            rows.iter().any(|row| {
                row.application_identity.as_ref().is_some_and(|identity| {
                    identity.display_name == *name && identity.has_icon_asset()
                })
            })
        });
        view.capture_evidence.mark_apps_identity_matrix_ready(ready);
    }
    if view.capture_evidence.system_about_requested() {
        view.page = TopPage::System;
        view.dashboard.section = SystemSection::Hardware;
        view.show_system_about();
        view.capture_evidence.mark_system_about_ready(
            view.window_surface_kind() == Some(WindowSurfaceKind::SystemAbout),
        );
    }
    if view.capture_evidence.about_requested() {
        view.page = TopPage::System;
        view.dashboard.section = SystemSection::Hardware;
        view.show_about();
        view.capture_evidence
            .mark_about_ready(view.window_surface_kind() == Some(WindowSurfaceKind::About));
    }
    if view.capture_evidence.first_run_requested() {
        view.first_run.info = Some(CaptureEvidence::first_run_fixture_info());
        view.first_run.phase = FirstRunPhase::Available;
        view.show_first_run();
        view.capture_evidence
            .mark_first_run_ready(view.first_run_open());
    }
    if view.capture_evidence.process_memory_pss_swap_requested() {
        view.page = TopPage::Apps;
        view.set_process_sort(
            taskmanager_shell::SortCol::Memory,
            taskmanager_shell::SortDir::Desc,
        );
        view.processes_state
            .hidden_cols
            .remove(&taskmanager_shell::SortCol::Swap);
    }
}

fn configure_category_apps(view: &mut RootView) {
    view.page = TopPage::Apps;
    view.set_process_sort(
        taskmanager_shell::SortCol::Cpu,
        taskmanager_shell::SortDir::Desc,
    );
    view.processes_state.expanded_apps.clear();
    view.processes_state
        .expanded_apps
        .insert(category_expansion_key(
            taskmanager_core::core::process::ProcessCategory::Application,
        ));
}

fn apply_inventory_capture(
    view: &mut RootView,
    services_updated: bool,
    startup_updated: bool,
    restore_startup_fixture: bool,
    cx: &mut Context<RootView>,
) {
    let service_capture_update =
        services_updated || view.capture_evidence.service_inventory_capture_requested();
    if let Some(service) = view.sync_capture_service_system(service_capture_update) {
        view.page = TopPage::Services;
        view.open_service_details(service);
        view.capture_evidence
            .mark_service_details_ready(view.service_details_target().is_some());
    }
    if view.capture_evidence.services_search_highlight_requested() {
        view.page = TopPage::Services;
        view.services_state.query = "4".to_owned();
        if let Some(input) = view.services_search.as_ref().cloned() {
            input.update(cx, |state, cx| state.set_value("4", cx));
        }
    }
    let startup_capture_update =
        startup_updated || view.capture_evidence.startup_inventory_capture_requested();
    if view.sync_capture_startup_system(
        startup_capture_update,
        restore_startup_fixture || startup_capture_update,
    ) {
        view.page = TopPage::Startup;
        let entries = view.startup_entries_rc().clone();
        view.capture_evidence
            .mark_startup_impact_ready(true, &entries);
        let evidence = view.startup_boot_evidence().cloned();
        view.capture_evidence
            .mark_startup_failure_evidence_ready(true, evidence.as_ref());
        view.capture_evidence.mark_startup_boot_markers_ready(
            true,
            view.startup_boot_evidence().is_some()
                && view.capture_evidence.startup_boot_baseline().is_some(),
        );
    }
}

fn apply_shell_capture(view: &mut RootView, cx: &mut Context<RootView>) {
    if let Some(snapshot) = view.capture_evidence.system_hardware_npu_fixture() {
        taskmanager_shell::fixture::seed_direct_track_fact(
            &mut view.shell,
            taskmanager_shell::fixture::DirectTrackSeedFact::NpuInventory(snapshot),
        );
        let revision = view.projection().system_revision;
        let snapshot = view.projection().npu_inventory.clone();
        view.materialized.replace_npu_inventory(revision, snapshot);
        let installed = view.npu_inventory().is_some_and(|inventory| {
            inventory.is_success()
                && inventory
                    .devices
                    .iter()
                    .any(|device| device.device_id.as_str() == "accel:capture-npu0")
        });
        view.capture_evidence
            .mark_system_npu_fixture_ready(installed);
    }
    let timestamp_ms = view.system_snapshot().timestamp_ms;
    if view.capture_evidence.seed_gpu_engine_inventory_history(
        &view.telemetry.system_history,
        &view.telemetry_ingestor,
        timestamp_ms,
    ) {
        view.page = TopPage::Performance;
        view.select_device(SelectedDevice::Gpu(0));
    }
    if view.capture_evidence.history_replay_open_requested() {
        view.page = TopPage::Performance;
        if view.history_replay_entry_available() {
            view.toggle_history_replay(cx);
            view.capture_evidence.note_history_replay_opened();
        }
    }
    if view.capture_evidence.diagnostic_preview_requested() {
        view.page = TopPage::System;
        view.open_diagnostic_preview();
        view.capture_evidence
            .mark_diagnostic_preview_ready(matches!(
                view.diagnostic_bundle_state(),
                Some(DiagnosticBundleUiState::Preview(_))
            ));
    }
    if view.capture_evidence.diagnostic_failure_requested() {
        view.page = TopPage::System;
        view.show_diagnostic_bundle_state(DiagnosticBundleUiState::Failed(
            taskmanager_core::core::DiagnosticBundleError::new(
                taskmanager_core::core::DiagnosticBundleErrorKind::Io,
            ),
        ));
        view.capture_evidence
            .mark_diagnostic_failure_ready(matches!(
                view.diagnostic_bundle_state(),
                Some(DiagnosticBundleUiState::Failed(_))
            ));
    }
    let panel = view.capture_evidence.on_dashboard_state(
        &mut view.dashboard,
        &view.telemetry.system_history,
        &view.telemetry_ingestor,
        timestamp_ms,
    );
    if let Some(events) = view.capture_evidence.take_event_history_fixture() {
        view.shell.replace_alert_event_history(events);
    }
    if let Some(panel) = panel {
        view.show_dashboard_panel(panel);
    }
}
