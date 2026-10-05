//! Deterministic full-surface demo frame construction: the `demo()` impl plus
//! its two honest fixtures (directory-usage + boot-evidence). Extracted from
//! `lib.rs` to keep the crate root under the source line budget; behavior is
//! unchanged — `demo_app()` stays reachable at `crate::demo_app` via a
//! `pub use` in `lib.rs`.

use crate::command_palette::TuiSurfaceAction;
use crate::health_review::HealthReviewMode;
use crate::ui::process_properties::{ProcessDetailsSection, ProcessPropertiesTarget};
use crate::{PerfDevice, TuiApp, TuiSurface};
use taskmanager_application::ProcessInsightFacet;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::first_run::FirstRunController;
use taskmanager_application::system_timeline::{SystemHistoryWindow, SystemPageSection};
use taskmanager_application::{AppAction, AppPage, InteractionEvent, PendingConfirmation};
use taskmanager_core::core::StorageDeviceKey;
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::diagnostics::{DiagnosticBundleError, DiagnosticBundleErrorKind};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::{DeviceGeneration, ProviderId};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessGroupScope,
};
use taskmanager_core::core::process_telemetry::{ContainerRollup, ContainerSummary, IsolationKind};
use taskmanager_core::core::smart::SmartSelfTestKind;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_core::core::time::{LocalTimeRules, LocalTimeRulesObservation};
use taskmanager_shell::fixture::alerts::seed_shell_active_alert;
use taskmanager_shell::fixture::dashboard_history::seed_shell_system_dashboard_history;
use taskmanager_shell::fixture::health::seed_shell_health;
use taskmanager_shell::fixture::process_insights::process_insights_projection;
use taskmanager_shell::fixture::process_insights::seed_process_properties_history;
use taskmanager_shell::fixture::process_tree::seed_shell_process_tree;
use taskmanager_shell::fixture::setup::setup_script_info;
use taskmanager_shell::fixture::smbios_memory::seed_shell_memory_inventory;
use taskmanager_shell::fixture::startup::startup_failure_evidence;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

pub(crate) mod capture;
use capture::prepare_capture_settings;
mod fixtures;
mod history;
mod power;
pub(crate) use fixtures::seed_fan_capture_sensors;
use fixtures::{
    demo_boot_evidence, demo_directory_usage, seed_alert_event_history_fixture,
    seed_capture_identity_matrix, seed_capture_storage_scenario, seed_demo_npu_inventory,
    seed_gpu_capture_history, seed_service_log_fixture,
};
use history::seed_demo_history;

impl TuiApp {
    /// A deterministic full-surface demo frame: the shared demo snapshot plus
    /// seeded containers. It has no configuration capability, so demo-mode
    /// settings can update local presentation but never touch a host file.
    #[must_use]
    pub fn demo() -> Self {
        use taskmanager_shell::demo_app;
        let mut app = Self::from_shell(demo_app());
        app.local_time_rules = LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0);
        seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::Containers(Some(ContainerRollup {
                state: DeviceState::healthy(1_785_292_800_000),
                containers: vec![
                    ContainerSummary {
                        id: "/docker/abc123".into(),
                        name: "postgres".into(),
                        runtime: Some(IsolationKind::Docker),
                        cgroup_path: "/docker/abc123".into(),
                        cpu_percentage: ScalarObservation::available(12.5, 1_785_292_800_000),
                        memory_bytes: ScalarObservation::available(
                            68 * 1024 * 1024 + 512 * 1024,
                            1_785_292_800_000,
                        ),
                        member_pids: vec![4201, 4202],
                    },
                    ContainerSummary {
                        id: "/docker/def456".into(),
                        name: "redis".into(),
                        runtime: Some(IsolationKind::Docker),
                        cgroup_path: "/docker/def456".into(),
                        cpu_percentage: ScalarObservation::available(3.1, 1_785_292_800_000),
                        memory_bytes: ScalarObservation::available(
                            24 * 1024 * 1024,
                            1_785_292_800_000,
                        ),
                        member_pids: vec![4301],
                    },
                ],
            })),
        );
        seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::StartupBootEvidence(Some(demo_boot_evidence())),
        );
        // Seed the SHARED slot the Disk panel renders (SystemProjectionStore, latest-wins
        // from `directory_usage_events`) — the same field a live platform
        // batch fills through the shell fold.
        seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::DirectoryUsage(Some(demo_directory_usage())),
        );
        seed_demo_npu_inventory(&mut app);
        seed_demo_history(&mut app);
        // The demo boot has no persisted appearance preference, so the shared
        // `TM_SKIN` testing override is the only appearance input here. When it
        // is unset/invalid the `ThemeParams::default()` (GNOME dark) set by
        // `from_shell` is preserved verbatim; production never reads it.
        if let Some(forced) = crate::theme::forced_theme_params_from_env() {
            app.theme_params = forced;
        }
        apply_capture_overrides(&mut app);
        app
    }
}

/// Total demo history depth (in frames) seeded for the CPU/Memory/disk/network
/// main charts. The demo runtime performs no live collection, so without this
/// seed every main graph would sit on the honest-but-useless "Collecting
/// samples…" cold-start placeholder forever (the shell demo frame records
/// exactly one sample). 36 frames fill one visible 60-sample window with the
/// same smooth measured-looking shape the GPU evidence scene uses.
pub(crate) const DEMO_HISTORY_FRAMES: usize = 36;

/// Seed a bounded measured-looking history for the demo frame's main charts by
/// replaying the canonical demo snapshot through the same typed ingestor the
/// shell's single cold-start frame uses, one correlated frame per second. The
/// swing around each seeded fact tapers linearly so the FINAL frame lands
/// exactly on the canonical projection values — the newest history sample can
/// never disagree with the snapshot the frame renders. This is deterministic
/// fixture data (like the GPU scene's five-frame seed), not a live collection.
/// Capture-only page and source-failure overrides. Normal demo launches keep
/// the complete healthy fixture; the evidence runner opts in through env vars
/// so a real terminal frame can prove the degraded list treatment.
fn apply_capture_overrides(app: &mut TuiApp) {
    let page_name = std::env::var("TM_TUI_CAPTURE_PAGE").ok();
    let device_name = std::env::var("TM_TUI_CAPTURE_DEVICE").ok();
    let scene_name = std::env::var("TM_TUI_CAPTURE_SCENE").ok();
    let failure_name = std::env::var("TM_TUI_CAPTURE_SOURCE_FAILURE").ok();
    let page = if scene_name.as_deref() == Some("system-npu") {
        Some(AppPage::System)
    } else if matches!(
        scene_name.as_deref(),
        Some(
            "process-force-kill"
                | "process-tree-confirm"
                | "process-batch-confirm"
                | "process-properties-performance"
                | "process-memory-pss-swap"
                | "process-network-details"
                | "process-gpu-details"
                | "process-resource-limits"
                | "process-isolation"
                | "apps-search-highlight"
                | "apps-group-expanded"
                | "apps-zero-gray"
                | "apps-identity-matrix"
                | "keyboard-focus"
                | "vertical-nav"
        )
    ) {
        Some(AppPage::Applications)
    } else if matches!(
        scene_name.as_deref(),
        Some("startup-impact" | "startup-failure-evidence" | "startup-boot-markers")
    ) {
        Some(AppPage::Startup)
    } else if matches!(
        scene_name.as_deref(),
        Some("service-details-logs" | "services-search-highlight")
    ) {
        Some(AppPage::Services)
    } else if matches!(
        scene_name.as_deref(),
        Some(
            "smart-self-test-confirm"
                | "telemetry-paused"
                | "sidebar-hidden"
                | "about"
                | "system-about"
                | "system-hardware"
                | "storage-health"
                | "sensor-center"
                | "system-dashboard"
                | "active-alert"
                | "alert-rules-manager"
                | "saved-view-presets"
                | "first-run"
                | "settings-permission-center"
                | "battery-fan-performance"
                | "battery-live-performance"
                | "device-hotplug"
                | "sidebar-edit"
                | "history-replay"
                | "history-60m"
                | "event-center"
        )
    ) {
        Some(AppPage::Performance)
    } else {
        page_name
            .as_deref()
            .or(failure_name.as_deref())
            .and_then(capture_page)
    };
    let Some(page) = page else {
        return;
    };
    app.shell.application.active_page = page;
    if page == AppPage::Performance
        && let Some(device) = device_name.as_deref().and_then(capture_device)
    {
        app.select_perf_device(device);
        match device {
            PerfDevice::Gpu => seed_gpu_capture_history(app),
            PerfDevice::Fan => seed_fan_capture_sensors(app),
            _ => {}
        }
    }
    if let Some(scene) = scene_name.as_deref() {
        apply_capture_scene_override(app, scene);
    }
    let Some(failure_page) = failure_name.as_deref().and_then(capture_page) else {
        return;
    };
    if failure_page != page {
        return;
    }
    let status = SourceStatus {
        provider: ProviderId::borrowed("capture.provider"),
        outcome: SourceOutcome::Unavailable(FailureKind::TimedOut),
        item_count: match page {
            AppPage::Services => app.shell.projection().services.as_ref().map_or(0, Vec::len),
            AppPage::Startup => app
                .shell
                .projection()
                .startup_entries
                .as_ref()
                .map_or(0, Vec::len),
            AppPage::Users => app.shell.projection().sessions.as_ref().map_or(0, Vec::len),
            _ => return,
        },
    };
    match page {
        AppPage::Services => seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::ServicesSource(Some(vec![status])),
        ),
        AppPage::Startup => seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::StartupSource(Some(vec![status])),
        ),
        AppPage::Users => seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::SessionsSource(Some(vec![status])),
        ),
        _ => {}
    }
}

fn seed_capture_process_target(app: &TuiApp) -> Option<FrozenProcessIdentity> {
    let first = app
        .shell
        .projection()
        .processes
        .as_ref()
        .and_then(|p| p.first())
        .cloned()?;
    FrozenProcessIdentity::from_process(&first)
}

fn seed_capture_multiple_process_targets(app: &TuiApp) -> Vec<FrozenProcessIdentity> {
    app.shell
        .projection()
        .processes
        .as_ref()
        .map(|p| {
            p.iter()
                .take(3)
                .filter_map(FrozenProcessIdentity::from_process)
                .collect()
        })
        .unwrap_or_default()
}

fn capture_page(name: &str) -> Option<AppPage> {
    match name {
        "performance" => Some(AppPage::Performance),
        "applications" => Some(AppPage::Applications),
        "services" => Some(AppPage::Services),
        "system" => Some(AppPage::System),
        "startup" => Some(AppPage::Startup),
        "users" => Some(AppPage::Users),
        "app-history" => Some(AppPage::AppHistory),
        _ => None,
    }
}

fn capture_device(name: &str) -> Option<PerfDevice> {
    match name {
        "cpu" => Some(PerfDevice::Cpu),
        "memory" => Some(PerfDevice::Memory),
        "disk" => Some(PerfDevice::Disk),
        "network" => Some(PerfDevice::Network),
        "gpu" => Some(PerfDevice::Gpu),
        "npu" => Some(PerfDevice::Npu),
        "battery" => Some(PerfDevice::Battery),
        "fan" => Some(PerfDevice::Fan),
        _ => None,
    }
}

pub(crate) fn persisted_history_capture_requested() -> bool {
    std::env::var("TM_TUI_CAPTURE_SCENE").is_ok_and(|scene| {
        matches!(
            scene.as_str(),
            "history-replay" | "application-history-replay"
        )
    })
}

pub fn demo_app() -> TuiApp {
    TuiApp::demo()
}

pub(crate) fn apply_capture_scene_override(app: &mut TuiApp, scene: &str) {
    if matches!(
        scene,
        "process-properties-performance"
            | "process-memory-pss-swap"
            | "process-network-details"
            | "process-gpu-details"
            | "process-resource-limits"
            | "process-isolation"
    ) {
        seed_process_properties_history(&mut app.shell);
    }
    match scene {
        "process-force-kill" => {
            app.shell.application.active_page = AppPage::Applications;
            if let Some(target) = seed_capture_process_target(app) {
                let intent = ProcessBatchIntent {
                    action: ProcessBatchAction::Kill,
                    scope: ProcessGroupScope::PidAdjacency,
                    targets: vec![target],
                };
                let _ =
                    app.shell
                        .application
                        .interaction
                        .reduce(InteractionEvent::ArmConfirmation(
                            PendingConfirmation::ProcessBatch(intent),
                        ));
            }
        }
        "process-tree-confirm" => {
            app.shell.application.active_page = AppPage::Applications;
            if let Some(root) = seed_shell_process_tree(&mut app.shell) {
                app.shell.request_process_tree_end(root);
            }
        }
        "process-batch-confirm" => {
            app.shell.application.active_page = AppPage::Applications;
            let targets = seed_capture_multiple_process_targets(app);
            if !targets.is_empty() {
                let intent = ProcessBatchIntent {
                    action: ProcessBatchAction::Kill,
                    scope: ProcessGroupScope::PidAdjacency,
                    targets,
                };
                let _ =
                    app.shell
                        .application
                        .interaction
                        .reduce(InteractionEvent::ArmConfirmation(
                            PendingConfirmation::ProcessBatch(intent),
                        ));
            }
        }
        "smart-self-test-confirm" => {
            app.shell.application.active_page = AppPage::Performance;
            app.select_perf_device(PerfDevice::Disk);
            let intent = SmartSelfTestIntent {
                device_id: "disk:demo:nvme0".into(),
                device_generation: DeviceGeneration::new(1),
                device_key: StorageDeviceKey::new("nvme0n1"),
                display_name: "TiPro9000 2TB".into(),
                kind: SmartSelfTestKind::Short,
            };
            app.shell.arm_smart_self_test(intent);
        }
        "about" => {
            app.shell.application.active_page = AppPage::Performance;
            app.toggle_about();
        }

        "system-about" => {
            app.open_system_information();
        }
        "system-dashboard" | "history-60m" => {
            app.shell.application.active_page = AppPage::System;
            app.select_system_section(SystemPageSection::Dashboard);
            app.system_history_window = if scene == "history-60m" {
                SystemHistoryWindow::SixtyMinutes
            } else {
                SystemHistoryWindow::FifteenMinutes
            };
            let _ = seed_shell_system_dashboard_history(&mut app.shell, 7_200_000);
        }
        "system-hardware" => {
            app.shell.application.active_page = AppPage::System;
            seed_shell_memory_inventory(&mut app.shell);
            app.memory_capture_scroll_pending = true;
        }

        "active-alert" => {
            app.shell.application.active_page = AppPage::Performance;
            let _ = seed_shell_active_alert(&mut app.shell);
        }
        "storage-health" | "sensor-center" => {
            seed_shell_health(&mut app.shell);
            app.toggle_health();
            let mode = if scene == "storage-health" {
                HealthReviewMode::Storage
            } else {
                HealthReviewMode::Sensors
            };
            let _ = app.run_surface_protocol_action(TuiSurfaceAction::SelectHealthReview(mode));
        }
        "alert-rules-manager" => {
            app.toggle_health();
        }
        "diagnostic-preview" => {
            app.shell.application.active_page = AppPage::Performance;
            app.open_diagnostic_bundle();
        }
        "diagnostic-failure" => {
            app.shell.application.active_page = AppPage::Performance;
            app.show_diagnostic_bundle(DiagnosticBundleUiState::Failed(
                DiagnosticBundleError::new(DiagnosticBundleErrorKind::Unavailable),
            ));
        }
        "smart-missing-tool"
        | "smart-permission"
        | "partition-disk-usage"
        | "partition-live-usage" => {
            app.shell.application.active_page = AppPage::Performance;
            app.select_perf_device(PerfDevice::Disk);
            if let Some(snapshot) = app.shell.projection().snapshot.as_ref() {
                let mut s = (*snapshot).clone();
                seed_capture_storage_scenario(&mut s, scene);
                seed_projection_fact(
                    &mut app.shell,
                    ProjectionSeedFact::Snapshot(Box::new(Some(s))),
                );
            }
        }
        "gpu-engine-inventory" | "intel-gpu-telemetry" => {
            app.shell.application.active_page = AppPage::Performance;
            app.select_perf_device(PerfDevice::Gpu);
            if let Some(snapshot) = app.shell.projection().snapshot.as_ref() {
                let mut s = (*snapshot).clone();
                seed_capture_storage_scenario(&mut s, scene);
                seed_projection_fact(
                    &mut app.shell,
                    ProjectionSeedFact::Snapshot(Box::new(Some(s))),
                );
            }
        }
        "process-selection" => {
            app.shell.application.active_page = AppPage::Applications;
            let index = (0..app.shell.visible_process_count())
                .find(|index| app.shell.row_identity_at(*index).is_some());
            if let Some(index) = index {
                let _ = app.shell.select_row(index);
                app.reconcile_applications_cursor();
            }
        }
        "process-properties-performance" => {
            app.shell.application.active_page = AppPage::Applications;
            if let Some(item) = app
                .shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned()
                && let Some(identity) = FrozenProcessIdentity::from_process(&item)
            {
                app.process_properties_view = Some(ProcessPropertiesTarget {
                    facet: ProcessInsightFacet::Network,
                    item,
                    section: ProcessDetailsSection::Performance,
                    scroll: 0,
                });
                let _ = app.shell.open_process_properties_for(identity);
            }
        }
        "process-memory-pss-swap" => {
            app.shell.application.active_page = AppPage::Applications;
            if let Some(item) = app
                .shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned()
                && let Some(identity) = FrozenProcessIdentity::from_process(&item)
            {
                app.process_properties_view = Some(ProcessPropertiesTarget {
                    facet: ProcessInsightFacet::Network,
                    item,
                    section: ProcessDetailsSection::Overview,
                    scroll: 7,
                });
                let _ = app.shell.open_process_properties_for(identity);
            }
        }

        "process-network-details"
        | "process-gpu-details"
        | "process-resource-limits"
        | "process-isolation" => {
            app.shell.application.active_page = AppPage::Applications;
            if let Some(item) = app
                .shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned()
                && let Some(identity) = FrozenProcessIdentity::from_process(&item)
            {
                app.process_properties_view = Some(ProcessPropertiesTarget {
                    facet: match scene {
                        "process-gpu-details" => ProcessInsightFacet::Gpu,
                        "process-resource-limits" => ProcessInsightFacet::Resources,
                        "process-isolation" => ProcessInsightFacet::Isolation,
                        _ => ProcessInsightFacet::Network,
                    },
                    item,
                    section: ProcessDetailsSection::Insights,
                    scroll: 0,
                });
                let _ = app.shell.open_process_properties_for(identity.clone());
                if let Some(projection) = process_insights_projection(identity) {
                    seed_projection_fact(
                        &mut app.shell,
                        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
                    );
                }
            }
        }
        "startup-impact" | "startup-failure-evidence" | "startup-boot-markers" => {
            app.shell.application.active_page = AppPage::Startup;
            if scene == "startup-failure-evidence" {
                seed_projection_fact(
                    &mut app.shell,
                    ProjectionSeedFact::StartupBootEvidence(Some(startup_failure_evidence(
                        3_600_000,
                    ))),
                );
            }
        }
        "service-details-logs" => {
            app.shell.application.active_page = AppPage::Services;
            seed_service_log_fixture(&mut app.shell);
        }
        "services-search-highlight" => {
            app.shell.application.active_page = AppPage::Services;
            app.shell.query = "Network".into();
        }
        "apps-search-highlight" => {
            app.shell.application.active_page = AppPage::Applications;
            app.shell.query = "zed".into();
        }
        "apps-group-expanded" => {
            app.shell.application.active_page = AppPage::Applications;
        }
        "apps-zero-gray" => {
            app.shell.application.active_page = AppPage::Applications;
            app.prefs.gray_zero = true;
        }
        "settings-zero-gray" | "settings-switch-focus" => {
            prepare_capture_settings(
                app,
                if scene == "settings-zero-gray" { 24 } else { 2 },
                scene == "settings-zero-gray",
            );
        }
        "apps-identity-matrix" => {
            app.shell.application.active_page = AppPage::Applications;
            if let Some(processes) = app.shell.projection().processes.as_ref() {
                let mut procs = (**processes).clone();
                seed_capture_identity_matrix(&mut procs);
                seed_projection_fact(&mut app.shell, ProjectionSeedFact::Processes(Some(procs)));
            }
        }
        "telemetry-paused" => {
            app.shell.application.active_page = AppPage::Performance;
            let _ = app.shell.apply_action(AppAction::TogglePause);
        }
        "sidebar-hidden" => {
            app.shell.application.active_page = AppPage::Performance;
        }
        "system-npu" => {
            // Paint clamps this intent to the last legal viewport, exercising the
            // same path a user reaches with PageDown.
            app.system_scroll = usize::MAX;
        }
        "history-replay" => {
            app.shell.application.active_page = AppPage::Performance;
            app.select_perf_device(PerfDevice::Cpu);
            app.open_history_replay();
        }
        "event-center" => {
            seed_alert_event_history_fixture(&mut app.shell);
            app.open_local_surface(TuiSurface::Health);
        }
        "settings-permission-center" => {
            app.open_local_surface(TuiSurface::Settings);
            app.settings_form.field = 30;
        }
        "first-run" => {
            app.first_run = FirstRunController::from_observation(Some(setup_script_info()));
            app.open_first_run();
        }
        "saved-view-presets" => {
            app.open_local_surface(TuiSurface::ColumnMenu { selection: 0 });
        }
        "sidebar-edit" => {
            prepare_capture_settings(app, 9, true);
        }
        "keyboard-focus" | "vertical-nav" => {
            app.shell.application.active_page = AppPage::Applications;
        }
        "battery-fan-performance" | "battery-live-performance" => {
            power::seed(&mut app.shell);
            if scene == "battery-fan-performance" {
                seed_fan_capture_sensors(app);
            }
            app.shell.application.active_page = AppPage::Performance;
            app.select_perf_device(PerfDevice::Battery);
        }
        "device-hotplug" => {
            app.shell.application.active_page = AppPage::Performance;
            app.select_perf_device(PerfDevice::Disk);
        }
        _ => {}
    }
}
