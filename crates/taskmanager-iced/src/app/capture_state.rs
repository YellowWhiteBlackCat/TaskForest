//! Capture-only marker lifecycle and target preparation for the Iced frontend.

use std::path::PathBuf;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::first_run::FirstRunController;
use taskmanager_core::core::diagnostics::{DiagnosticBundleError, DiagnosticBundleErrorKind};
use taskmanager_shell::fixture::setup::setup_script_info;

use taskmanager_application::{AppAction, AppPage, InteractionEvent, PendingConfirmation};
use taskmanager_core::core::SmartSelfTestKind;
use taskmanager_core::core::StorageDeviceKey;
use taskmanager_core::core::history::HistoryWindow;
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId};
use taskmanager_core::core::metrics::DiskMetrics;
use taskmanager_core::core::process::{ProcessBatchAction, ProcessBatchIntent, ProcessGroupScope};
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

use super::capture_fixtures::*;
pub(super) use super::capture_fixtures::{capture_device_from_name, capture_page_from_name};
use super::{DetailsSection, IcedApp, LocalSurface, Message, PerfDevice};

pub(super) struct CaptureState {
    pub(super) marker: Option<PathBuf>,
    pub(super) emitted: bool,
}

impl CaptureState {
    pub(super) const fn new(marker: Option<PathBuf>) -> Self {
        Self {
            marker,
            emitted: false,
        }
    }
}

/// Apply one fixed capture target and its page-local facts.
pub(super) fn apply_capture_target(app: &mut IcedApp, target: &str) {
    if apply_capture_surface_and_process(app, target) {
        return;
    }
    if apply_capture_hardware_and_perf(app, target) {
        return;
    }
    if let Some(page) = capture_page_from_name(target) {
        app.shell.application.active_page = page;
        if page == AppPage::System {
            seed_capture_npu_fixture(app);
        }
    } else if let Some(device) = capture_device_from_name(target) {
        if matches!(device, PerfDevice::Npu(_)) {
            seed_capture_npu_fixture(app);
        }
        app.performance.selected_device = device;
    }
}

fn apply_capture_surface_and_process(app: &mut IcedApp, target: &str) -> bool {
    if target == "service-details" {
        app.shell.application.active_page = AppPage::Services;
        let _ = app.open_service_details_for_effect(0);
    } else if target == crate::capture::HEALTH_TARGET
        || target == "system-dashboard"
        || target == "sensor-center"
    {
        let _ = app.update(Message::OpenHealth);
    } else if target == "about" || target == "system-about" || target == "system-hardware" {
        app.open_local_surface(LocalSurface::About);
    } else if target == "settings" {
        app.open_local_surface(LocalSurface::Settings);
    } else if target == "containers" {
        app.open_local_surface(LocalSurface::Containers);
    } else if target == "alerts" || target == "active-alert" || target == "alert-rules-manager" {
        app.open_local_surface(LocalSurface::AlertCenter);
    } else if target == "first-run" {
        app.first_run = FirstRunController::from_observation(Some(setup_script_info()));
        app.open_local_surface(LocalSurface::FirstRun);
    } else if target == "run-task" {
        app.open_local_surface(LocalSurface::RunTask);
    } else if target == "disk-smart" {
        app.open_local_surface(LocalSurface::DiskSmart { index: 0 });
    } else if target == "diagnostic-preview" {
        app.open_diagnostic_bundle();
    } else if target == "diagnostic-failure" {
        app.open_local_surface(LocalSurface::DiagnosticBundle(
            DiagnosticBundleUiState::Failed(DiagnosticBundleError::new(
                DiagnosticBundleErrorKind::Unavailable,
            )),
        ));
    } else if target == "process-details" {
        app.shell.application.active_page = AppPage::Applications;
        seed_capture_process_details(app, DetailsSection::Overview);
    } else if target == "process-properties-performance" {
        app.shell.application.active_page = AppPage::Applications;
        seed_capture_process_details(app, DetailsSection::Performance);
    } else if target == "process-insights"
        || target == "process-network-details"
        || target == "process-gpu-details"
        || target == "process-resource-limits"
        || target == "process-isolation"
    {
        app.shell.application.active_page = AppPage::Applications;
        seed_capture_process_details(app, DetailsSection::Insights);
    } else if target == "process-memory-pss-swap" {
        app.shell.application.active_page = AppPage::Applications;
        seed_capture_process_details(app, DetailsSection::Overview);
    } else if target == "process-command" {
        app.shell.application.active_page = AppPage::Applications;
        seed_capture_process_details(app, DetailsSection::Command);
    } else if target == "process-affinity" {
        app.shell.application.active_page = AppPage::Applications;
        if let Some(target_proc) = seed_capture_process_target(app) {
            app.open_local_surface(LocalSurface::ProcessAffinity {
                target: target_proc,
            });
        }
    } else if target == "process-end-confirm" {
        app.shell.application.active_page = AppPage::Applications;
        if let Some(target_proc) = seed_capture_process_target(app) {
            let _ = app
                .shell
                .application
                .interaction
                .reduce(InteractionEvent::ArmConfirmation(
                    PendingConfirmation::EndTask(target_proc),
                ));
        }
    } else if target == "process-force-kill" {
        app.shell.application.active_page = AppPage::Applications;
        if let Some(target_proc) = seed_capture_process_target(app) {
            let intent = ProcessBatchIntent {
                action: ProcessBatchAction::Kill,
                scope: ProcessGroupScope::PidAdjacency,
                targets: vec![target_proc],
            };
            let _ = app
                .shell
                .application
                .interaction
                .reduce(InteractionEvent::ArmConfirmation(
                    PendingConfirmation::ProcessBatch(intent),
                ));
        }
    } else if target == "process-tree-confirm" {
        app.shell.application.active_page = AppPage::Applications;
        if let Some(target_proc) = seed_capture_process_target(app) {
            let intent = ProcessBatchIntent {
                action: ProcessBatchAction::EndProcessTree,
                scope: ProcessGroupScope::PidAdjacency,
                targets: vec![target_proc],
            };
            let _ = app
                .shell
                .application
                .interaction
                .reduce(InteractionEvent::ArmConfirmation(
                    PendingConfirmation::ProcessBatch(intent),
                ));
        }
    } else if target == "process-batch-confirm" {
        app.shell.application.active_page = AppPage::Applications;
        let targets = seed_capture_multiple_process_targets(app);
        if !targets.is_empty() {
            let intent = ProcessBatchIntent {
                action: ProcessBatchAction::Kill,
                scope: ProcessGroupScope::PidAdjacency,
                targets,
            };
            let _ = app
                .shell
                .application
                .interaction
                .reduce(InteractionEvent::ArmConfirmation(
                    PendingConfirmation::ProcessBatch(intent),
                ));
        }
    } else if target == "apps-search-highlight" {
        app.shell.application.active_page = AppPage::Applications;
        app.shell.query = "zed".into();
    } else if target == "apps-group-expanded" {
        app.shell.application.active_page = AppPage::Applications;
    } else if target == "apps-zero-gray" {
        app.shell.application.active_page = AppPage::Applications;
        app.configuration.preferences_mut().gray_zero_values = true;
    } else if target == "apps-identity-matrix" {
        app.shell.application.active_page = AppPage::Applications;
        if let Some(processes) = app.shell.projection().processes.as_ref() {
            let mut procs = (**processes).clone();
            seed_capture_identity_matrix(&mut procs);
            seed_projection_fact(&mut app.shell, ProjectionSeedFact::Processes(Some(procs)));
        }
    } else if target == "saved-view-presets" {
        app.shell.application.active_page = AppPage::Applications;
        app.open_process_columns_menu();
    } else if target == "keyboard-focus" || target == "vertical-nav" {
        app.shell.application.active_page = AppPage::Applications;
    } else {
        return false;
    }
    true
}

fn apply_capture_hardware_and_perf(app: &mut IcedApp, target: &str) -> bool {
    if target == "smart-self-test-confirm" {
        app.shell.application.active_page = AppPage::Performance;
        app.performance.selected_device = PerfDevice::Disk(0);
        if let Some(disk) = app
            .shell
            .projection()
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.disks.first())
        {
            let intent = SmartSelfTestIntent {
                device_id: DeviceId::new(disk.device_id.clone()),
                device_generation: disk.device_generation,
                device_key: StorageDeviceKey::new(disk.name.clone()),
                display_name: if disk.model.is_empty() {
                    disk.name.clone()
                } else {
                    disk.model.clone()
                },
                kind: SmartSelfTestKind::Short,
            };
            app.shell.arm_smart_self_test(intent);
        }
    } else if target == "services-search-highlight" {
        app.shell.application.active_page = AppPage::Services;
        let _ = app.update(Message::ServicesSearchChanged("Network".into()));
    } else if target == "service-details-logs" {
        app.shell.application.active_page = AppPage::Services;
        seed_service_log_fixture(&mut app.shell);
    } else if target == "startup-impact"
        || target == "startup-failure-evidence"
        || target == "startup-boot-markers"
    {
        app.shell.application.active_page = AppPage::Startup;
    } else if target == "telemetry-paused" {
        app.shell.application.active_page = AppPage::Performance;
        let _ = app.shell.apply_action(AppAction::TogglePause);
    } else if target == "sidebar-hidden" {
        app.shell.application.active_page = AppPage::Performance;
        app.performance.sidebar_visible = false;
    } else if target == "smart-missing-tool"
        || target == "smart-permission"
        || target == "partition-disk-usage"
        || target == "partition-live-usage"
    {
        app.shell.application.active_page = AppPage::Performance;
        app.performance.selected_device = PerfDevice::Disk(0);
        if let Some(snapshot) = app.shell.projection().snapshot.as_ref() {
            let mut s = (*snapshot).clone();
            seed_capture_storage_scenario(&mut s, target);
            seed_projection_fact(
                &mut app.shell,
                ProjectionSeedFact::Snapshot(Box::new(Some(s))),
            );
        }
    } else if target == "gpu-engine-inventory" || target == "intel-gpu-telemetry" {
        app.shell.application.active_page = AppPage::Performance;
        app.performance.selected_device = PerfDevice::Gpu(0);
        if let Some(snapshot) = app.shell.projection().snapshot.as_ref() {
            let mut s = (*snapshot).clone();
            seed_capture_storage_scenario(&mut s, target);
            seed_projection_fact(
                &mut app.shell,
                ProjectionSeedFact::Snapshot(Box::new(Some(s))),
            );
        }
    } else if target == "settings-zero-gray" || target == "settings-switch-focus" {
        app.open_local_surface(LocalSurface::Settings);
        app.configuration.preferences_mut().gray_zero_values = true;
    } else if target == "history-replay" || target == "history-60m" {
        app.shell.application.active_page = AppPage::Performance;
        app.seed_capture_history_replay();
        if target == "history-60m" {
            app.select_history_replay_window(HistoryWindow::OneHour);
        }
    } else if target == "application-history-replay" {
        app.shell.application.active_page = AppPage::AppHistory;
    } else if target == "battery-fan-performance" || target == "battery-live-performance" {
        app.shell.application.active_page = AppPage::Performance;
        app.performance.selected_device = PerfDevice::Battery(0);
        seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::PowerSupplies(Some(dynamic_power_fixture())),
        );
        if target == "battery-fan-performance" {
            seed_projection_fact(
                &mut app.shell,
                ProjectionSeedFact::Sensors(Some(dynamic_sensor_fixture())),
            );
        }
    } else if target == "device-hotplug" {
        app.shell.application.active_page = AppPage::Performance;
        app.performance.sidebar_visible = true;
        if let Some(snapshot) = app.shell.projection().snapshot.as_ref() {
            let mut s = (*snapshot).clone();
            let mut disk = DiskMetrics::new("/dev/sdb");
            disk.device_id = "disk:hotplug:usb0".into();
            disk.disk_type = "USB Drive".into();
            disk.model = "TaskForest Flash".into();
            disk.device_generation = DeviceGeneration::new(2);
            s.disks.push(disk);
            seed_projection_fact(
                &mut app.shell,
                ProjectionSeedFact::Snapshot(Box::new(Some(s))),
            );
        }
    } else if target == "sidebar-edit" {
        app.shell.application.active_page = AppPage::Performance;
        app.performance.sidebar_visible = true;
    } else if target == "event-center" {
        app.shell
            .replace_alert_event_history(capture_event_fixture());
        app.open_local_surface(LocalSurface::AlertCenter);
    } else if target == "settings-permission-center" {
        app.open_local_surface(LocalSurface::Settings);
    } else {
        return false;
    }
    true
}
