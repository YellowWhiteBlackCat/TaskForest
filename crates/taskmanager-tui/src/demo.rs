//! Deterministic full-surface demo frame construction: the `demo()` impl plus
//! its two honest fixtures (directory-usage + boot-evidence). Extracted from
//! `lib.rs` to keep the crate root under the source line budget; behavior is
//! unchanged — `demo_app()` stays reachable at `crate::demo_app` via a
//! `pub use` in `lib.rs`.

use crate::ui::process_properties::{ProcessDetailsSection, ProcessPropertiesTarget};
use crate::{PerfDevice, TuiApp};
use taskmanager_application::{
    AppAction, AppPage, InteractionEvent, PendingConfirmation, ProcessInsightsProjection,
    ProcessInsightsRevision,
};
use taskmanager_core::core::StorageDeviceKey;
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::{DeviceGeneration, ProviderId};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::metrics::ScalarObservationGroup;
use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessGroupScope,
};
use taskmanager_core::core::process_telemetry::{ContainerRollup, ContainerSummary, IsolationKind};
use taskmanager_core::core::smart::SmartSelfTestKind;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_core::core::time::{LocalTimeRules, LocalTimeRulesObservation};
use taskmanager_shell::fixture::{
    ProjectionSeedFact, record_demo_history_frame, seed_projection_fact,
};

mod fixtures;
pub(crate) use fixtures::seed_fan_capture_sensors;
use fixtures::{
    demo_boot_evidence, demo_directory_usage, seed_demo_npu_inventory, seed_gpu_capture_history,
    seed_service_log_fixture,
};

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
fn seed_demo_history(app: &mut TuiApp) {
    let Some(base) = app.projection().snapshot.clone() else {
        return;
    };
    let last_frame = DEMO_HISTORY_FRAMES.saturating_sub(1);
    for frame_index in 1..DEMO_HISTORY_FRAMES {
        let index = frame_index as f64;
        let settle = 1.0 - index / f64::from(last_frame as u32);
        // Three non-harmonic phases so no two channels draw the same wave.
        let wave = |period: f64, phase: f64| {
            (0.5 - 0.5 * (std::f64::consts::TAU * index / period + phase).cos()) * settle
        };
        let mut frame = base.clone();
        frame.timestamp_ms = base.timestamp_ms.saturating_add(frame_index as u64 * 1_000);

        // CPU: per-core utilization swings proportionally to its own base
        // (busy cores breathe more), the global readout on its own phase.
        let mut cpu_observations = frame.cpu.scalar_observations().clone();
        if let Some(cores) = cpu_observations.core_usage_group.current_observations() {
            let varied: Vec<f32> = cores
                .iter()
                .filter_map(|core| core.current_value())
                .map(|base_value| {
                    let base = f64::from(*base_value);
                    let amplitude = 3.0 + base * 0.25;
                    let varied = base + amplitude * wave(12.0, 0.0);
                    varied.clamp(0.5, 99.0) as f32
                })
                .collect();
            cpu_observations.core_usage_group =
                ScalarObservationGroup::available(varied, frame.timestamp_ms);
        }
        if let Some(global) = cpu_observations.global_usage_pct.current_value() {
            let varied = f64::from(*global) + 14.0 * wave(17.0, 0.9);
            cpu_observations.global_usage_pct =
                ScalarObservation::available(varied.clamp(1.0, 99.0) as f32, frame.timestamp_ms);
        }
        frame.cpu.apply_scalar_observations(cpu_observations);

        // Memory: the used share breathes on a slow phase; the available lane
        // follows the same bounded total so the gauge stays honest.
        let memory = &mut frame.memory;
        let mut scalar = *memory.scalar_observations();
        let optional = memory.optional_observations().clone();
        if let (Some(total), Some(used)) = (
            scalar.total_bytes.current_value().copied(),
            scalar.used_bytes.current_value().copied(),
        ) {
            let headroom = total.saturating_sub(1);
            let used_varied = (used.min(headroom) as f64
                + (64.0 * 1024.0 * 1024.0) * wave(19.0, 1.7))
            .clamp(1024.0, headroom as f64);
            let used_bytes = (used_varied as u64).min(headroom);
            scalar.used_bytes = ScalarObservation::available(used_bytes, frame.timestamp_ms);
            scalar.available_bytes =
                ScalarObservation::available(total - used_bytes, frame.timestamp_ms);
        }
        if let Some(swap_used) = scalar.swap_used_bytes.current_value() {
            let varied = *swap_used as f64 + (96.0 * 1024.0 * 1024.0) * wave(9.0, 2.4);
            scalar.swap_used_bytes =
                ScalarObservation::available(varied.max(0.0) as u64, frame.timestamp_ms);
        }
        memory.apply_observations(scalar, optional);

        // Disk and NIC main lanes breathe on their own phases so the device
        // trend rows and throughput summaries draw a shape, not a flat line.
        for disk in &mut frame.disks {
            let mut scalar = *disk.scalar_observations();
            if let Some(read) = scalar.read_bytes_per_sec.current_value() {
                let varied = *read as f64 * (0.65 + 0.7 * wave(11.0, 0.4));
                scalar.read_bytes_per_sec =
                    ScalarObservation::available(varied.max(0.0) as u64, frame.timestamp_ms);
            }
            if let Some(write) = scalar.write_bytes_per_sec.current_value() {
                let varied = *write as f64 * (0.65 + 0.7 * wave(8.0, 1.1));
                scalar.write_bytes_per_sec =
                    ScalarObservation::available(varied.max(0.0) as u64, frame.timestamp_ms);
            }
            disk.apply_scalar_observations(scalar);
        }
        for network in &mut frame.networks {
            let mut scalar = *network.scalar_observations();
            let wireless = network.wireless_observations().clone();
            if let Some(rx) = scalar.rx_bytes_per_sec.current_value() {
                let varied = *rx as f64 * (0.55 + 0.9 * wave(13.0, 2.0));
                scalar.rx_bytes_per_sec =
                    ScalarObservation::available(varied.max(0.0) as u64, frame.timestamp_ms);
            }
            if let Some(tx) = scalar.tx_bytes_per_sec.current_value() {
                let varied = *tx as f64 * (0.55 + 0.9 * wave(10.0, 0.2));
                scalar.tx_bytes_per_sec =
                    ScalarObservation::available(varied.max(0.0) as u64, frame.timestamp_ms);
            }
            network.apply_observations(network.adapter_type(), scalar, wireless);
        }

        record_demo_history_frame(&mut app.shell, &frame, None, None);
    }
}

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
    } else if matches!(scene_name.as_deref(), Some("application-history-replay")) {
        Some(AppPage::AppHistory)
    } else if matches!(
        scene_name.as_deref(),
        Some("diagnostic-preview" | "diagnostic-failure")
    ) {
        Some(AppPage::System)
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
                | "history-replay"
                | "history-60m"
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
    match scene_name.as_deref() {
        Some("process-force-kill") => {
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
        Some("process-tree-confirm") => {
            app.shell.application.active_page = AppPage::Applications;
            if let Some(target) = seed_capture_process_target(app) {
                let intent = ProcessBatchIntent {
                    action: ProcessBatchAction::EndProcessTree,
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
        Some("process-batch-confirm") => {
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
        Some("smart-self-test-confirm") => {
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
        Some("about" | "system-about" | "system-hardware") => {
            app.shell.application.active_page = AppPage::Performance;
            app.toggle_about();
        }
        Some(
            "storage-health"
            | "sensor-center"
            | "system-dashboard"
            | "active-alert"
            | "alert-rules-manager",
        ) => {
            app.shell.application.active_page = AppPage::Performance;
            app.toggle_health();
        }
        Some("process-properties-performance") => {
            app.shell.application.active_page = AppPage::Applications;
            if let Some(item) = app
                .shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned()
            {
                if let Some(identity) = FrozenProcessIdentity::from_process(&item) {
                    app.process_properties_view = Some(ProcessPropertiesTarget {
                        item,
                        section: ProcessDetailsSection::Performance,
                        scroll: 0,
                    });
                    let _ = app.shell.open_process_properties_for(identity);
                }
            }
        }
        Some("process-memory-pss-swap") => {
            app.shell.application.active_page = AppPage::Applications;
            if let Some(item) = app
                .shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned()
            {
                if let Some(identity) = FrozenProcessIdentity::from_process(&item) {
                    app.process_properties_view = Some(ProcessPropertiesTarget {
                        item,
                        section: ProcessDetailsSection::Overview,
                        scroll: 0,
                    });
                    let _ = app.shell.open_process_properties_for(identity);
                }
            }
        }
        Some(
            "process-network-details"
            | "process-gpu-details"
            | "process-resource-limits"
            | "process-isolation",
        ) => {
            app.shell.application.active_page = AppPage::Applications;
            if let Some(item) = app
                .shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned()
            {
                if let Some(identity) = FrozenProcessIdentity::from_process(&item) {
                    app.process_properties_view = Some(ProcessPropertiesTarget {
                        item,
                        section: ProcessDetailsSection::Insights,
                        scroll: 0,
                    });
                    let _ = app.shell.open_process_properties_for(identity.clone());
                    let revision = ProcessInsightsRevision::new(1);
                    let mut tracker = ProcessInsightsProjection::default();
                    tracker.begin(identity, revision);
                    if let Some(projection) = tracker.snapshot() {
                        seed_projection_fact(
                            &mut app.shell,
                            ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
                        );
                    }
                }
            }
        }
        Some("startup-impact" | "startup-failure-evidence" | "startup-boot-markers") => {
            app.shell.application.active_page = AppPage::Startup;
        }
        Some("service-details-logs") => {
            app.shell.application.active_page = AppPage::Services;
            seed_service_log_fixture(&mut app.shell);
        }
        Some("services-search-highlight") => {
            app.shell.application.active_page = AppPage::Services;
            app.shell.query = "Network".into();
        }
        Some("apps-search-highlight") => {
            app.shell.application.active_page = AppPage::Applications;
            app.shell.query = "zed".into();
        }
        Some("apps-group-expanded") => {
            app.shell.application.active_page = AppPage::Applications;
        }
        Some("telemetry-paused") => {
            app.shell.application.active_page = AppPage::Performance;
            let _ = app.shell.apply_action(AppAction::TogglePause);
        }
        Some("sidebar-hidden") => {
            app.shell.application.active_page = AppPage::Performance;
        }
        Some("history-replay" | "history-60m") => {
            app.shell.application.active_page = AppPage::Performance;
        }
        Some("application-history-replay") => {
            app.shell.application.active_page = AppPage::AppHistory;
        }
        Some("diagnostic-preview" | "diagnostic-failure") => {
            app.shell.application.active_page = AppPage::System;
        }
        Some("system-npu") => {
            // Paint clamps this intent to the last legal viewport, exercising the
            // same path a user reaches with PageDown.
            app.system_scroll = usize::MAX;
        }
        _ => {}
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

/// Deterministic full-surface demo frame (containers included).
#[must_use]
pub fn demo_app() -> TuiApp {
    TuiApp::demo()
}
