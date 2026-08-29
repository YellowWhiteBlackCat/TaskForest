//! Deterministic full-surface demo frame construction: the `demo()` impl plus
//! its two honest fixtures (directory-usage + boot-evidence). Extracted from
//! `lib.rs` to keep the crate root under the source line budget; behavior is
//! unchanged — `demo_app()` stays reachable at `crate::demo_app` via a
//! `pub use` in `lib.rs`.

use crate::{PerfDevice, TuiApp};
use taskmanager_application::{
    AppPage, CorrelatedEvent, NpuInventoryEvent, PlatformEventBatch, PlatformEventContext,
};
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::directory_usage::DirectoryUsageSnapshot;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId, ProviderId};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::npu::{
    NpuDevice, NpuEngineKind, NpuEngineUsage, NpuInventorySnapshot, NpuMemoryReport,
};
use taskmanager_core::core::process_telemetry::{ContainerRollup, ContainerSummary, IsolationKind};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::core::startup::StartupBootEvidenceSnapshot;
use taskmanager_platform_contract::{CapabilityId, EventSequence, RequestId};

impl TuiApp {
    /// A deterministic full-surface demo frame: the shared demo snapshot plus
    /// seeded containers. It has no configuration capability, so demo-mode
    /// settings can update local presentation but never touch a host file.
    #[must_use]
    pub fn demo() -> Self {
        let mut app = Self::from_shell(taskmanager_shell::demo_app());
        app.local_time_rules = taskmanager_core::core::time::LocalTimeRulesObservation::current(
            taskmanager_core::core::time::LocalTimeRules::utc(),
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
        seed_demo_history(&mut app);
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
                taskmanager_core::core::metrics::ScalarObservationGroup::available(
                    varied,
                    frame.timestamp_ms,
                );
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

        taskmanager_shell::fixture::record_demo_history_frame(&mut app.shell, &frame, None, None);
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
    use taskmanager_core::core::metrics::{GpuEngine, GpuEngineKind};

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
/// snapshot types come directly from their owner modules in
/// `taskmanager-core`.
fn demo_directory_usage() -> DirectoryUsageSnapshot {
    use taskmanager_core::core::directory_usage::{
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
        unreadable: Some(taskmanager_core::core::failure::FailureKind::PermissionDenied),
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
    use taskmanager_core::core::startup::StartupCriticalChainNode;
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
