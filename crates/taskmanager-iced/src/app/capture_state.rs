//! Capture-only marker lifecycle and target preparation for the Iced frontend.

use crate::ui::system_dashboard::SystemDashboardMessage;
use std::path::PathBuf;
use taskmanager_application::ProcessInsightFacet;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::first_run::FirstRunController;
use taskmanager_application::system_timeline::{SystemHistoryWindow, SystemPageSection};
use taskmanager_core::core::diagnostics::{DiagnosticBundleError, DiagnosticBundleErrorKind};
use taskmanager_shell::fixture::alerts::seed_shell_active_alert;
use taskmanager_shell::fixture::dashboard_history::seed_shell_system_dashboard_history;
use taskmanager_shell::fixture::health::seed_shell_health;
use taskmanager_shell::fixture::setup::setup_script_info;
use taskmanager_shell::fixture::smbios_memory::seed_shell_memory_inventory;
use taskmanager_shell::fixture::startup::startup_failure_evidence;
use taskmanager_shell::presentation::health_review::HealthReviewSection;

use taskmanager_application::{AppAction, AppPage, InteractionEvent, PendingConfirmation};
use taskmanager_core::core::SmartSelfTestKind;
use taskmanager_core::core::StorageDeviceKey;
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId};
use taskmanager_core::core::metrics::DiskMetrics;
use taskmanager_core::core::process::{ProcessBatchAction, ProcessBatchIntent, ProcessGroupScope};
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

use super::capture_fixtures::*;
pub(super) use super::capture_fixtures::{capture_device_from_name, capture_page_from_name};
use super::{
    AlertsMessage, DetailsSection, FocusTarget, IcedApp, LocalSurface, Message, PerfDevice,
    SettingsChange,
};
use taskmanager_shell::fixture::process_tree::seed_shell_process_tree;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum CapturePresentationFrame {
    #[default]
    WaitingForData,
    WaitingForPresentation,
    Presented,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum CaptureDataTarget {
    #[default]
    General,
    PerformanceHistory,
    ApplicationHistory,
    MemoryInventory,
    SystemDashboard,
    Health,
    ActiveAlerts,
    AlertRules,
    Battery,
    Smart(&'static str),
    Focus,
    StartupFailure,
    ProcessProperties(&'static str),
}

pub(super) struct CaptureState {
    pub(super) marker: Option<PathBuf>,
    pub(super) emitted: bool,
    pub(super) presentation_frame: CapturePresentationFrame,
    pub(super) data_target: CaptureDataTarget,
    pub(super) scroll_requested: bool,
    pub(super) focus_target: Option<FocusTarget>,
    pub(super) focus_scheduled: bool,
    pub(super) focus_presented: bool,
}

impl CaptureState {
    pub(super) const fn new(marker: Option<PathBuf>) -> Self {
        Self {
            marker,
            emitted: false,
            presentation_frame: CapturePresentationFrame::WaitingForData,
            data_target: CaptureDataTarget::General,
            scroll_requested: false,
            focus_target: None,
            focus_scheduled: false,
            focus_presented: false,
        }
    }
}

/// Apply one fixed capture target and its page-local facts.
pub(super) fn apply_capture_target(app: &mut IcedApp, target: &str) {
    app.capture.data_target = match target {
        "history-replay" => CaptureDataTarget::PerformanceHistory,
        "application-history-replay" => CaptureDataTarget::ApplicationHistory,
        "system-hardware" => CaptureDataTarget::MemoryInventory,
        "system-dashboard" | "history-60m" => CaptureDataTarget::SystemDashboard,
        "storage-health" | "sensor-center" => CaptureDataTarget::Health,
        "active-alert" => CaptureDataTarget::ActiveAlerts,
        "alert-rules-manager" => CaptureDataTarget::AlertRules,
        "battery-fan-performance" | "battery-live-performance" => CaptureDataTarget::Battery,
        "smart-missing-tool" => CaptureDataTarget::Smart("smart-missing-tool"),
        "smart-permission" => CaptureDataTarget::Smart("smart-permission"),
        "settings-switch-focus" | "settings-zero-gray" | "keyboard-focus" | "sidebar-edit" => {
            CaptureDataTarget::Focus
        }
        "startup-failure-evidence" => CaptureDataTarget::StartupFailure,
        "process-properties-performance" => {
            CaptureDataTarget::ProcessProperties("process-properties-performance")
        }
        "process-memory-pss-swap" => {
            CaptureDataTarget::ProcessProperties("process-memory-pss-swap")
        }
        "process-network-details" => {
            CaptureDataTarget::ProcessProperties("process-network-details")
        }
        "process-gpu-details" => CaptureDataTarget::ProcessProperties("process-gpu-details"),
        "process-resource-limits" => {
            CaptureDataTarget::ProcessProperties("process-resource-limits")
        }
        "process-isolation" => CaptureDataTarget::ProcessProperties("process-isolation"),

        _ => CaptureDataTarget::General,
    };
    if matches!(target, "storage-health" | "sensor-center") {
        prepare_health_capture(app, target);
        return;
    }
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

fn prepare_health_capture(app: &mut IcedApp, target: &str) {
    seed_shell_health(&mut app.shell);
    let _ = app.update(Message::SelectPage(AppPage::System));
    let _ = app.update(Message::SystemDashboard(
        SystemDashboardMessage::SelectSection(SystemPageSection::Health),
    ));
    let section = if target == "sensor-center" {
        HealthReviewSection::Sensors
    } else {
        HealthReviewSection::Storage
    };
    let _ = app.update(Message::SystemDashboard(
        SystemDashboardMessage::SelectHealthSection(section),
    ));
}

fn apply_capture_surface_and_process(app: &mut IcedApp, target: &str) -> bool {
    if apply_capture_alerts(app, target) {
        return true;
    }
    if target == "process-selection" {
        app.shell.application.active_page = AppPage::Applications;
        let index = (0..app.shell.visible_process_count())
            .find(|index| app.shell.row_identity_at(*index).is_some());
        if let Some(index) = index {
            let _ = app.update(Message::SelectRow(index));
        }
    } else if target == "service-details" {
        app.shell.application.active_page = AppPage::Services;
        let _ = app.open_service_details_for_effect(0);
    } else if matches!(target, "system-dashboard" | "history-60m") {
        app.shell.application.active_page = AppPage::System;
        app.system_section = SystemPageSection::Dashboard;
        let _ = seed_shell_system_dashboard_history(&mut app.shell, 7_200_000);
        app.system_dashboard_window = if target == "history-60m" {
            SystemHistoryWindow::SixtyMinutes
        } else {
            SystemHistoryWindow::FifteenMinutes
        };
    } else if target == crate::capture::HEALTH_TARGET {
        let _ = app.update(Message::OpenHealth);
    } else if target == "about" {
        app.open_local_surface(LocalSurface::About);
    } else if target == "system-about" {
        let _ = app.update(Message::OpenSystemInformation);
    } else if target == "system-hardware" {
        app.shell.application.active_page = AppPage::System;
        seed_shell_memory_inventory(&mut app.shell);
    } else if target == "settings" {
        app.open_local_surface(LocalSurface::Settings);
    } else if target == "containers" {
        app.open_local_surface(LocalSurface::Containers);
    } else if target == "alerts" {
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
        app.process_presentation.insights_facet = match target {
            "process-gpu-details" => ProcessInsightFacet::Gpu,
            "process-resource-limits" => ProcessInsightFacet::Resources,
            "process-isolation" => ProcessInsightFacet::Isolation,
            _ => ProcessInsightFacet::Network,
        };
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
        if let Some(root) = seed_shell_process_tree(&mut app.shell) {
            app.shell.request_process_tree_end(root);
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
        if target == "keyboard-focus" {
            app.capture.focus_target = Some(FocusTarget::PageTab(AppPage::Applications));
        }
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
        if target == "startup-failure-evidence" {
            seed_projection_fact(
                &mut app.shell,
                ProjectionSeedFact::StartupBootEvidence(Some(startup_failure_evidence(3_600_000))),
            );
        }
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
        if matches!(target, "smart-missing-tool" | "smart-permission") {
            let _ = app.update(Message::OpenDiskSmart { index: 0 });
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
        let _ = app.update(Message::SettingsChanged(SettingsChange::GrayZeroValues(
            true,
        )));
        app.capture.focus_target = Some(FocusTarget::SettingsChoice {
            section: if target == "settings-zero-gray" {
                "zero-values"
            } else {
                "hc"
            },
            index: 0,
        });
    } else if target == "history-replay" {
        app.shell.application.active_page = AppPage::Performance;
    } else if target == "application-history-replay" {
        app.shell.application.active_page = AppPage::AppHistory;
    } else if target == "battery-fan-performance" || target == "battery-live-performance" {
        app.shell.application.active_page = AppPage::Performance;
        app.performance.selected_device = PerfDevice::Battery(0);
        seed_capture_dynamic_history(app);
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
        let _ = app.update(Message::OpenSettings);
        app.capture.focus_target = Some(FocusTarget::SettingsChoice {
            section: "device-memory",
            index: 0,
        });
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

fn apply_capture_alerts(app: &mut IcedApp, target: &str) -> bool {
    if !matches!(target, "active-alert" | "alert-rules-manager") {
        return false;
    }
    if target == "active-alert" {
        let _ = seed_shell_active_alert(&mut app.shell);
    }
    let _ = app.update(Message::Alerts(AlertsMessage::OpenPage));
    true
}
