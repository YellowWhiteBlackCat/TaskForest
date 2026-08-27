//! Deterministic full-surface demo frame construction: the `demo()` impl plus
//! its two honest fixtures (directory-usage + boot-evidence). Extracted from
//! `lib.rs` to keep the crate root under the source line budget; behavior is
//! unchanged — `demo_app()` stays reachable at `crate::demo_app` via a
//! `pub use` in `lib.rs`.

use crate::{PerfDevice, TuiApp};
use taskmanager_application::{
    AppPage, CapabilityId, ContainerRollup, ContainerSummary, CorrelatedEvent, DeviceGeneration,
    DeviceId, DeviceState, DirectoryUsageSnapshot, EventSequence, FailureKind, IsolationKind,
    NpuDevice, NpuEngineKind, NpuEngineUsage, NpuInventoryEvent, NpuInventorySnapshot,
    NpuMemoryReport, PlatformEventBatch, PlatformEventContext, ProviderId, RequestId,
    ScalarObservation, SourceOutcome, SourceStatus, StartupBootEvidenceSnapshot,
};

impl TuiApp {
    /// A deterministic full-surface demo frame: the shared demo snapshot plus
    /// seeded containers. It has no configuration capability, so demo-mode
    /// settings can update local presentation but never touch a host file.
    #[must_use]
    pub fn demo() -> Self {
        let mut app = Self::from_shell(taskmanager_shell::demo_app());
        app.local_time_rules = taskmanager_application::LocalTimeRulesObservation::current(
            taskmanager_application::LocalTimeRules::utc(),
            0,
        );
        taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::Containers(Some(ContainerRollup {
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
        taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::StartupBootEvidence(Some(
                demo_boot_evidence(),
            )),
        );
        // Seed the SHARED slot the Disk panel renders (SystemProjectionStore, latest-wins
        // from `directory_usage_events`) — the same field a live platform
        // batch fills through the shell fold.
        taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::DirectoryUsage(Some(
                demo_directory_usage(),
            )),
        );
        seed_demo_npu_inventory(&mut app);
        apply_capture_overrides(&mut app);
        app
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
        if device == PerfDevice::Gpu {
            seed_gpu_capture_history(app);
        }
    }
    if scene_name.as_deref() == Some("system-npu") {
        // Paint clamps this intent to the last legal viewport, exercising the
        // same path a user reaches with PageDown.
        app.system_scroll = usize::MAX;
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
        AppPage::Services => taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::ServicesSource(Some(vec![status])),
        ),
        AppPage::Startup => taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::StartupSource(Some(vec![status])),
        ),
        AppPage::Users => taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::SessionsSource(Some(vec![status])),
        ),
        _ => {}
    }
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
        "battery" => Some(PerfDevice::Battery),
        "fan" => Some(PerfDevice::Fan),
        _ => None,
    }
}

fn seed_demo_npu_inventory(app: &mut TuiApp) {
    const OBSERVED_AT_MS: u64 = 1_785_292_800_000;
    let engines = NpuEngineKind::ALL
        .iter()
        .copied()
        .enumerate()
        .map(|(index, kind)| NpuEngineUsage {
            kind,
            utilization_pct: ScalarObservation::available(
                11.0 + (index as f32 * 7.0),
                OBSERVED_AT_MS,
            ),
        })
        .collect();
    let inventory = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("accel0"),
            device_generation: DeviceGeneration::new(1),
            brand: Some("Intel AI Boost".into()),
            driver: Some("intel_vpu".into()),
            utilization_pct: ScalarObservation::available(44.0, OBSERVED_AT_MS),
            engines,
            memory: NpuMemoryReport {
                dedicated_total_bytes: ScalarObservation::available(
                    512 * 1024 * 1024,
                    OBSERVED_AT_MS,
                ),
                shared_total_bytes: ScalarObservation::available(
                    4 * 1024 * 1024 * 1024,
                    OBSERVED_AT_MS,
                ),
            },
        }],
        OBSERVED_AT_MS,
    );
    let context = PlatformEventContext {
        request_id: RequestId::MIN,
        capability: CapabilityId::ACCELERATOR_NPU,
        provider: Some(ProviderId::borrowed("fixture.npu")),
        sequence: EventSequence::new(1),
        observed_at_ms: OBSERVED_AT_MS,
    };
    let mut batch = PlatformEventBatch::default();
    batch.npu_inventory_events.push(CorrelatedEvent::new(
        context,
        NpuInventoryEvent::Update(inventory),
    ));
    app.apply_platform_batch(batch);
}

/// Capture-only measured GPU sequence. Normal demo construction keeps its
/// single cold-start sample; the explicit GPU evidence scene adds five typed
/// frames and two real engine series so pixel review sees chart/viewport
/// behavior rather than a collecting placeholder.
fn seed_gpu_capture_history(app: &mut TuiApp) {
    use taskmanager_application::{GpuEngine, GpuEngineKind};

    let Some(mut snapshot) = app.projection().snapshot.clone() else {
        return;
    };
    let Some(gpu) = snapshot.gpu.first_mut() else {
        return;
    };
    gpu.engines = vec![
        GpuEngine {
            name: "Render/3D".into(),
            kind: GpuEngineKind::Render,
            usage_pct: 0.0,
        },
        GpuEngine {
            name: "Video Decode".into(),
            kind: GpuEngineKind::VideoDecode,
            usage_pct: 0.0,
        },
    ];
    for (index, (utilization, render, video)) in [
        (12.0, 9.0, 3.0),
        (27.0, 21.0, 8.0),
        (46.0, 39.0, 12.0),
        (34.0, 28.0, 7.0),
        (61.0, 52.0, 18.0),
    ]
    .into_iter()
    .enumerate()
    {
        let observed_at_ms = 1_785_292_800_100_u64.saturating_add(index as u64 * 1_000);
        snapshot.timestamp_ms = observed_at_ms;
        let gpu = &mut snapshot.gpu[0];
        let mut observations = *gpu.scalar_observations();
        observations.utilization_pct = ScalarObservation::available(utilization, observed_at_ms);
        gpu.apply_scalar_observations(observations);
        gpu.engines[0].usage_pct = render;
        gpu.engines[1].usage_pct = video;
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &snapshot,
            None,
            None,
        );
    }
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
}

/// Deterministic full-surface demo frame (containers included).
#[must_use]
pub fn demo_app() -> TuiApp {
    TuiApp::demo()
}

/// Deterministic directory-usage fixture for the demo frame: a `Completed`
/// scan of `/var` with one readable subtree (measured size + file count) and
/// one unreadable subtree (`PermissionDenied`, so the renderer must show a
/// danger dash — never a fabricated 0 B or the untrustworthy number), plus
/// capped totals carrying one unreadable directory. This is honest fixture
/// data, not a fabricated provider answer; it exercises the same
/// `SystemProjectionStore::directory_usage` slot the live platform-batch fold fills (the
/// snapshot types reach this firewalled crate through the application
/// boundary's public `model` module — ADR-020).
fn demo_directory_usage() -> DirectoryUsageSnapshot {
    use taskmanager_application::{
        DirectoryScanId, DirectoryScanStatus, DirectoryScanTotals, DirectoryUsageEntry,
    };
    let observed_at_ms = 1_785_292_800_000;
    let readable = DirectoryUsageEntry {
        path: "lib/postgres".into(),
        depth: 1,
        size_bytes: ScalarObservation::available(2 * 1024 * 1024 * 1024, observed_at_ms),
        file_count: ScalarObservation::available(4200, observed_at_ms),
        unreadable: None,
    };
    let unreadable = DirectoryUsageEntry {
        path: "cache/private".into(),
        depth: 1,
        // A measured value the renderer must NOT print: the unreadable flag
        // forces a danger dash, proving the panel never fabricates this size
        // as a "0 B" stand-in or leaks the untrustworthy number.
        size_bytes: ScalarObservation::available(7 * 1024 * 1024 * 1024, observed_at_ms),
        file_count: ScalarObservation::available(900, observed_at_ms),
        unreadable: Some(taskmanager_application::FailureKind::PermissionDenied),
    };
    DirectoryUsageSnapshot {
        scan_id: DirectoryScanId::new(1),
        root: "/var".into(),
        status: DirectoryScanStatus::Completed,
        entries: vec![readable, unreadable],
        totals: DirectoryScanTotals {
            directories_visited: 7,
            files_counted: 42,
            unreadable_directories: 1,
            bytes_counted: ScalarObservation::available(2 * 1024 * 1024 * 1024, observed_at_ms),
            depth_reached: 1,
            capped: true,
        },
    }
}

/// Deterministic boot-evidence fixture for the demo frame: a measured
/// systemd-user critical chain (three timed units, one untimed node) so the
/// Startup-page waterfall renders in demo mode exactly like a measured boot.
/// This is honest fixture data, not a fabricated provider answer.
fn demo_boot_evidence() -> StartupBootEvidenceSnapshot {
    use taskmanager_application::StartupCriticalChainNode;
    let healthy = DeviceState::healthy(1_785_292_800_000);
    StartupBootEvidenceSnapshot {
        state: healthy,
        failed_units_state: healthy,
        critical_chain_state: healthy,
        failed_units_failure: None,
        critical_chain_failure: None,
        failed_units: Vec::new(),
        critical_chain: vec![
            StartupCriticalChainNode {
                unit: "dbus.service".into(),
                activated_at_ms: Some(500),
                duration_ms: Some(1_200),
            },
            StartupCriticalChainNode {
                unit: "network-online.target".into(),
                activated_at_ms: Some(1_700),
                duration_ms: Some(900),
            },
            StartupCriticalChainNode {
                unit: "graphical.target".into(),
                activated_at_ms: None,
                duration_ms: None,
            },
            StartupCriticalChainNode {
                unit: "multi-user.target".into(),
                activated_at_ms: Some(2_600),
                duration_ms: Some(2_500),
            },
        ],
    }
}
