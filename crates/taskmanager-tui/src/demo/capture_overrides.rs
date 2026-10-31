//! Environment-selected, deterministic capture overrides for TUI demo scenes.
//! Normal demo construction remains in the parent module; this module owns the
//! capture-only page/device selectors and scene fixture preparation.

use crate::command_palette::TuiSurfaceAction;
use crate::health_review::HealthReviewMode;
use crate::ui::process_properties::{ProcessDetailsSection, ProcessPropertiesTarget};
use crate::{PerfDevice, TuiApp, TuiSurface};
use taskforest_core::core::diagnostics::{DiagnosticBundleError, DiagnosticBundleErrorKind};
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::identity::ProviderId;
use taskforest_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessGroupScope,
    ProcessMetadataObservation, ProcessMetadataObservations,
};
use taskforest_core::core::smart::SmartSelfTestKind;
use taskforest_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_application::ProcessInsightFacet;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::first_run::FirstRunController;
use taskmanager_application::system_timeline::{SystemHistoryWindow, SystemPageSection};
use taskmanager_application::{AppAction, AppPage, InteractionEvent, PendingConfirmation};
use taskmanager_shell::SearchSlot;
use taskmanager_shell::fixture::alerts::seed_shell_active_alert;
use taskmanager_shell::fixture::dashboard_history::seed_shell_system_dashboard_history;
use taskmanager_shell::fixture::health::{seed_shell_health, seed_smart_self_test_capabilities};
use taskmanager_shell::fixture::process_insights::{
    process_insights_projection, seed_process_properties_history,
};
use taskmanager_shell::fixture::process_tree::seed_shell_process_tree;
use taskmanager_shell::fixture::setup::setup_script_info;
use taskmanager_shell::fixture::smbios_memory::seed_shell_memory_inventory;
use taskmanager_shell::fixture::startup::startup_failure_evidence;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
use taskmanager_shell::presentation::smart::self_test_intent;

use super::capture::{self, prepare_capture_settings};
use super::fixtures;
use super::fixtures::{
    seed_alert_event_history_fixture, seed_capture_identity_matrix, seed_capture_storage_scenario,
    seed_fan_capture_sensors, seed_gpu_capture_history, seed_service_log_fixture,
};
use super::power;

/// Capture-only page and source-failure overrides. Normal demo launches keep
/// the complete healthy fixture; the evidence runner opts in through env vars
/// so a real terminal frame can prove the degraded list treatment.
pub(super) fn apply_capture_overrides(app: &mut TuiApp) {
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
                | "apps-empty"
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
    } else if scene_name.as_deref() == Some("users-session-confirmation") {
        Some(AppPage::Users)
    } else if matches!(
        scene_name.as_deref(),
        Some(
            "smart-self-test-confirm"
                | "telemetry-paused"
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
                | "health-review-events"
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
                    collateral: Default::default(),
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
                    collateral: Default::default(),
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
            seed_smart_self_test_capabilities(&mut app.shell);
            app.shell.application.active_page = AppPage::Performance;
            app.select_perf_device(PerfDevice::Disk);
            if let Some(intent) = app
                .sidebar_disks()
                .iter()
                .find_map(|disk| self_test_intent(disk, SmartSelfTestKind::Short))
            {
                let _ = app.shell.arm_smart_self_test(intent);
            }
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
            app.toggle_health();
            let _ = app.run_surface_protocol_action(TuiSurfaceAction::SelectHealthReview(
                HealthReviewMode::ActiveAlerts,
            ));
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
            app.shell.set_services_query("Network");
            app.shell.open_search_for(SearchSlot::Services);
        }
        "apps-search-highlight" => {
            app.shell.application.active_page = AppPage::Applications;
            app.shell.query = "zed".into();
        }
        "apps-group-expanded" => capture::prepare_expanded_tree(app),
        "keyboard-help" | "keyboard-help-end" => {
            capture::prepare_help(app, scene == "keyboard-help-end")
        }
        "apps-zero-gray" => capture::prepare_zero_gray(app),
        "apps-empty" => capture::prepare_empty_processes(app),
        "settings-zero-gray" | "settings-switch-focus" => {
            prepare_capture_settings(
                app,
                if scene == "settings-zero-gray" { 24 } else { 2 },
                scene == "settings-zero-gray",
            );
        }
        "apps-identity-matrix" => {
            app.shell.application.active_page = AppPage::Applications;
            let selected_pid = app
                .shell
                .visible_processes()
                .first()
                .map(|process| process.pid);
            if let Some(processes) = app.shell.projection().processes.as_ref() {
                let mut procs = (**processes).clone();
                seed_capture_identity_matrix(&mut procs);
                if let Some(pid) = selected_pid
                    && let Some(process) = procs.iter_mut().find(|process| process.pid == pid)
                {
                    process.cmdline = "/usr/bin/identity-matrix".into();
                    process.apply_metadata_observations(ProcessMetadataObservations {
                        executable_path: ProcessMetadataObservation::available(
                            "/usr/bin/identity-matrix-real".into(),
                            1,
                        ),
                        ..Default::default()
                    });
                }
                seed_projection_fact(&mut app.shell, ProjectionSeedFact::Processes(Some(procs)));
            }
            if !app.shell.visible_processes().is_empty() {
                let _ = app.shell.select_row(0);
                app.reconcile_applications_cursor();
            }
        }
        "telemetry-paused" => {
            app.shell.application.active_page = AppPage::Performance;
            let _ = app.shell.apply_action(AppAction::TogglePause);
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
        "event-center" | "health-review-events" => {
            seed_alert_event_history_fixture(&mut app.shell);
            app.open_local_surface(TuiSurface::Health);
            let _ = app.run_surface_protocol_action(TuiSurfaceAction::SelectHealthReview(
                HealthReviewMode::Events,
            ));
        }
        "users-session-confirmation" => {
            capture::prepare_users_session_confirmation(app);
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
            prepare_capture_settings(app, 0, false);
            let _ = crate::runtime::handle_key(
                app,
                ratatui::crossterm::event::KeyEvent::new(
                    ratatui::crossterm::event::KeyCode::F(4),
                    ratatui::crossterm::event::KeyModifiers::NONE,
                ),
            );
            let _ = crate::runtime::handle_key(
                app,
                ratatui::crossterm::event::KeyEvent::new(
                    ratatui::crossterm::event::KeyCode::F(2),
                    ratatui::crossterm::event::KeyModifiers::NONE,
                ),
            );
        }
        "sidebar-edit" => {
            prepare_capture_settings(app, 9, false);
            crate::runtime::handle_key(
                app,
                ratatui::crossterm::event::KeyEvent::new(
                    ratatui::crossterm::event::KeyCode::F(3),
                    ratatui::crossterm::event::KeyModifiers::NONE,
                ),
            );
        }
        "keyboard-focus" => {
            apply_capture_scene_override(app, "process-selection");
            app.cycle_focus_panel();
        }
        "vertical-nav" => {
            app.shell.application.active_page = AppPage::Applications;
            let _ = crate::runtime::handle_key(
                app,
                ratatui::crossterm::event::KeyEvent::new(
                    ratatui::crossterm::event::KeyCode::F(7),
                    ratatui::crossterm::event::KeyModifiers::NONE,
                ),
            );
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
            fixtures::seed_hotplug(&mut app.shell);
            app.shell.application.active_page = AppPage::Performance;
            app.select_perf_device(PerfDevice::Disk);
        }
        _ => {}
    }
}
