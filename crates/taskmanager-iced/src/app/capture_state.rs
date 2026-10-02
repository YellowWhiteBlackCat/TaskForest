//! Capture-only marker lifecycle and target preparation for the Iced frontend.

use std::path::PathBuf;

use taskmanager_application::{
    AppPage, InteractionEvent, PendingConfirmation, ProcessInsightsProjection,
    ProcessInsightsRevision,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::npu::{
    NpuDevice, NpuEngineKind, NpuEngineUsage, NpuInventorySnapshot, NpuMemoryReport,
};
use taskmanager_core::core::process::FrozenProcessIdentity;
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
    } else if target == crate::capture::HEALTH_TARGET {
        let _ = app.update(Message::OpenHealth);
    } else if target == "about" {
        app.open_local_surface(LocalSurface::About);
    } else if target == "settings" {
        app.open_local_surface(LocalSurface::Settings);
    } else if target == "containers" {
        app.open_local_surface(LocalSurface::Containers);
    } else if target == "alerts" {
        app.open_local_surface(LocalSurface::AlertCenter);
    } else if target == "first-run" {
        app.open_local_surface(LocalSurface::FirstRun);
    } else if target == "process-details" {
        app.shell.application.active_page = AppPage::Applications;
        seed_capture_process_details(app, DetailsSection::Overview);
    } else if target == "process-properties-performance" {
        app.shell.application.active_page = AppPage::Applications;
        seed_capture_process_details(app, DetailsSection::Performance);
    } else if target == "process-insights" {
        app.shell.application.active_page = AppPage::Applications;
        seed_capture_process_details(app, DetailsSection::Insights);
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
        let _ = app.open_service_details_for_effect(0);
    } else if target == "process-affinity" {
        app.shell.application.active_page = AppPage::Applications;
        if let Some(target_proc) = seed_capture_process_target(app) {
            app.open_local_surface(LocalSurface::ProcessAffinity {
                target: target_proc,
            });
        }
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
