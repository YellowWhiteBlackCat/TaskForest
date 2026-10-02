//! Capture-only marker lifecycle and target preparation for the Iced frontend.

use std::path::PathBuf;

use taskmanager_application::{
    AppAction, AppPage, InteractionEvent, PendingConfirmation, ProcessInsightsProjection,
    ProcessInsightsRevision,
};
use taskmanager_core::core::SmartSelfTestKind;
use taskmanager_core::core::StorageDeviceKey;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::npu::{
    NpuDevice, NpuEngineKind, NpuEngineUsage, NpuInventorySnapshot, NpuMemoryReport,
};
use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessGroupScope,
};
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

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
        app.open_local_surface(LocalSurface::FirstRun);
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
    } else if target == "process-end-confirm" {
        app.shell.application.active_page = AppPage::Applications;
        if let Some(target_proc) = seed_capture_process_target(app) {
            app.shell.application.selected_process = Some(target_proc.clone());
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
            app.shell.application.selected_process = Some(target_proc.clone());
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
            app.shell.application.selected_process = Some(target_proc.clone());
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
    } else if target == "smart-self-test-confirm" {
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
    } else if target == "apps-search-highlight" {
        app.shell.application.active_page = AppPage::Applications;
        app.shell.query = "zed".into();
    } else if target == "services-search-highlight" {
        app.shell.application.active_page = AppPage::Services;
        let _ = app.update(Message::ServicesSearchChanged("Network".into()));
    } else if target == "run-task" {
        app.open_local_surface(LocalSurface::RunTask);
    } else if target == "disk-smart" {
        app.open_local_surface(LocalSurface::DiskSmart { index: 0 });
    } else if target == "service-details-logs" {
        app.shell.application.active_page = AppPage::Services;
        seed_service_log_fixture(&mut app.shell);
    } else if target == "process-affinity" {
        app.shell.application.active_page = AppPage::Applications;
        if let Some(target_proc) = seed_capture_process_target(app) {
            app.open_local_surface(LocalSurface::ProcessAffinity {
                target: target_proc,
            });
        }
    } else if target == "startup-impact"
        || target == "startup-failure-evidence"
        || target == "startup-boot-markers"
    {
        app.shell.application.active_page = AppPage::Startup;
    } else if target == "telemetry-paused" {
        app.shell.application.active_page = AppPage::Performance;
        let _ = app.shell.apply_action(AppAction::TogglePause);
    } else if target == "apps-group-expanded" {
        app.shell.application.active_page = AppPage::Applications;
    } else if target == "sidebar-hidden" {
        app.shell.application.active_page = AppPage::Performance;
        app.performance.sidebar_visible = false;
    } else if target == "history-replay" || target == "history-60m" {
        app.shell.application.active_page = AppPage::Performance;
    } else if target == "application-history-replay" {
        app.shell.application.active_page = AppPage::AppHistory;
    } else if target == "diagnostic-preview" || target == "diagnostic-failure" {
        app.shell.application.active_page = AppPage::System;
    } else if let Some(page) = capture_page_from_name(target) {
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

pub(super) fn seed_capture_process_target(app: &IcedApp) -> Option<FrozenProcessIdentity> {
    let first = app
        .shell
        .projection()
        .processes
        .as_ref()
        .and_then(|processes| processes.first())
        .cloned()?;
    FrozenProcessIdentity::from_process(&first)
}

fn seed_capture_multiple_process_targets(app: &IcedApp) -> Vec<FrozenProcessIdentity> {
    app.shell
        .projection()
        .processes
        .as_ref()
        .map(|processes| {
            processes
                .iter()
                .take(3)
                .filter_map(FrozenProcessIdentity::from_process)
                .collect()
        })
        .unwrap_or_default()
}

fn seed_capture_process_details(app: &mut IcedApp, section: DetailsSection) {
    if let Some(target) = seed_capture_process_target(app) {
        app.shell.application.selected_process = Some(target.clone());
        let _ = app.shell.open_process_properties_for(target.clone());
        app.process_presentation.details_section = section;
        if section == DetailsSection::Insights {
            seed_capture_insights_fixture(app, &target);
        }
    }
}

fn seed_capture_insights_fixture(app: &mut IcedApp, target: &FrozenProcessIdentity) {
    let revision = ProcessInsightsRevision::new(1);
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target.clone(), revision);
    if let Some(projection) = tracker.snapshot() {
        seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
        );
    }
}

fn seed_service_log_fixture(shell: &mut ShellApp) {
    use taskmanager_core::core::services::{
        ServiceLogEntry, ServiceLogLevel, ServiceLogLevelFilter, ServiceLogQuery,
        ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter,
    };
    let Some(service) = shell.sorted_services().first().cloned() else {
        return;
    };
    let service_id = service.id.clone();
    let _ = shell.open_service_log_for(service_id.clone());
    let lines: &[&str] = &[
        "Started Network Manager.",
        "Reached target Network.",
        "wlan0: link becomes ready",
        "Starting Network Manager Script Dispatcher Service...",
        "Started Network Manager Script Dispatcher Service.",
        "dhcp: lease renewed (3600s)",
        "wlan0: Gained IPv6LL",
    ];
    let base_micros = 1_700_000_000_000_000;
    let entries: Vec<ServiceLogEntry> = lines
        .iter()
        .enumerate()
        .map(|(index, message)| ServiceLogEntry {
            cursor: format!("demo:{index:04}"),
            realtime_timestamp_micros: Some(base_micros + index as u64 * 1_500_000),
            priority: Some(6),
            level: ServiceLogLevel::Unknown,
            message: (*message).to_owned(),
        })
        .collect();
    let query = ServiceLogQuery {
        service_id: service_id.clone(),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };
    let snapshot = ServiceLogStreamSnapshot {
        query: query.clone(),
        state: ServiceLogStreamState::from_query_entries(&query, entries),
    };
    if let Some(open) = shell.service_log.as_mut() {
        open.feed.apply_at(snapshot, 1_700_000_000_000);
    }
}

pub(super) fn seed_capture_npu_fixture(app: &mut IcedApp) {
    let observed_at_ms = 7_000;
    let inventory = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("accel0"),
            brand: Some("Intel AI Boost".into()),
            driver: Some("intel_vpu".into()),
            utilization_pct: ScalarObservation::available(38.0, observed_at_ms),
            engines: vec![NpuEngineUsage {
                kind: NpuEngineKind::Matrix,
                utilization_pct: ScalarObservation::available(61.0, observed_at_ms),
            }],
            memory: NpuMemoryReport {
                dedicated_total_bytes: ScalarObservation::available(0, observed_at_ms),
                shared_total_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
                sram_total_bytes: ScalarObservation::available(32 * 1024 * 1024, observed_at_ms),
            },
            ..Default::default()
        }],
        observed_at_ms,
    );
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::NpuInventory(Some(inventory)),
    );
}

pub(super) fn capture_device_from_name(name: &str) -> Option<PerfDevice> {
    match name {
        "cpu" => Some(PerfDevice::Cpu),
        "memory" => Some(PerfDevice::Memory),
        "disk" => Some(PerfDevice::Disk(0)),
        "network" => Some(PerfDevice::Network(0)),
        "gpu" => Some(PerfDevice::Gpu(0)),
        "npu" => Some(PerfDevice::Npu(0)),
        "battery" => Some(PerfDevice::Battery(0)),
        "fan" => Some(PerfDevice::Fan(0)),
        _ => None,
    }
}

pub(super) fn capture_page_from_name(name: &str) -> Option<AppPage> {
    match name {
        "applications" => Some(AppPage::Applications),
        "services" => Some(AppPage::Services),
        "startup" => Some(AppPage::Startup),
        "users" => Some(AppPage::Users),
        "system" => Some(AppPage::System),
        "app-history" => Some(AppPage::AppHistory),
        _ => None,
    }
}
