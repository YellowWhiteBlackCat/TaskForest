//! Tests for the per-process GPU engines CLI projection: envelope wiring,
//! honest cold-start rates, and the Linux /proc fdinfo bulk-reader fixtures.

#[cfg(target_os = "linux")]
#[test]
fn accel_device_targets_qualify_for_the_fdinfo_walk() {
    assert!(is_drm_render_target("/dev/accel/accel0"));
    assert!(is_drm_render_target("/dev/dri/renderD128"));
    assert!(!is_drm_render_target("/dev/video0"));
    assert!(!is_drm_render_target("/dev/accelerometer0"));
}

#[test]
fn hardware_and_npu_flow_into_the_envelope_verbatim() {
    use taskmanager_core::core::hardware::{ComputeTopology, HardwareInfo};
    use taskmanager_core::core::npu::{NpuDevice, NpuInventorySnapshot};
    use taskmanager_core::{CpuInstructionFeature, DeviceId};

    let hardware = HardwareInfo::from_fragments(
        Default::default(),
        Default::default(),
        ComputeTopology {
            instruction_features: vec![CpuInstructionFeature::AvxVnni],
            ..ComputeTopology::default()
        },
        Default::default(),
    );
    let npu = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("accel0"),
            driver: Some("fixture".into()),
            ..NpuDevice::default()
        }],
        7,
    );
    let json = render_json_snapshot(
        &SystemSnapshot::default(),
        &[],
        &[],
        Some(&hardware),
        Some(&npu),
        7,
    );
    let value: serde_json::Value = serde_json::from_str(&json).expect("envelope must parse");
    assert_eq!(
        value["hardware"]["instruction_features"][0],
        serde_json::json!(CpuInstructionFeature::AvxVnni)
    );
    assert_eq!(value["npu_inventory"]["devices"][0]["device_id"], "accel0");
}

use super::*;
#[cfg(target_os = "linux")]
use taskmanager_core::core::device_state::DeviceState;

/// Build a synthetic per-process GPU breakdown exactly as the bulk reader
/// produces on a host with one GPU process holding one render engine:
/// cold-start rate (`Unavailable` — no delta on a single tick) and an
/// observed cumulative counter. No `/proc` is needed, mirroring how the
/// other CLI tests build snapshots by hand.
fn one_gpu_proc(pid: u32, engine_ns: u64, now_ms: u64) -> OwnedProcessGpuEngines {
    let mut breakdown = ProcessGpuEngines::empty_healthy(now_ms);
    breakdown.engines.push(ProcessGpuEngineUsage {
        name: "render".into(),
        usage_pct: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        engine_time_ns: ScalarObservation::available(engine_ns, now_ms),
        engine_cycles: ScalarObservation::default(),
    });
    (pid, breakdown)
}

fn render_with_extras(extras: ExportExtras<'_>) -> String {
    snapshot_to_json_with_extras(&SystemSnapshot::default(), &[], extras)
}

#[test]
fn gpu_proc_entries_populate_the_envelope_with_an_honest_cold_start_rate() {
    // The envelope the CLI emits when a GPU process is present must carry a
    // non-empty process_gpu_engines array — closing wave-1 Track A's honest
    // empty gap — and the single-tick rate must stay the typed gap, never 0.
    let now_ms = 5_000_u64;
    let owned = vec![one_gpu_proc(42, 750_000, now_ms)];
    let entries = build_process_gpu_entries(&owned);
    let extras = ExportExtras {
        containers: &[],
        process_gpu_engines: &entries,
        suggested_thresholds: &[],
        hardware: None,
        npu_inventory: None,
    };
    let json = render_with_extras(extras);
    let value: serde_json::Value = serde_json::from_str(&json).expect("envelope JSON must parse");

    let arr = &value["process_gpu_engines"];
    assert_eq!(
        arr.as_array().map(Vec::len),
        Some(1),
        "a GPU proc must populate process_gpu_engines, not leave it empty"
    );
    assert_eq!(arr[0]["pid"], 42);

    let engine = &arr[0]["engines"]["engines"][0];
    assert_eq!(engine["name"], "render");
    // Single-tick cold start: the rate is honestly unavailable, never 0.
    assert_eq!(
        engine["usage_pct"]["availability"]["status"], "unavailable",
        "the rate must be the typed cold-start gap, never a fabricated 0%"
    );
    assert!(engine["usage_pct"]["value"].is_null());
    // The cumulative counter IS observed on the only tick the CLI takes.
    assert_eq!(
        engine["engine_time_ns"]["availability"]["status"],
        "available"
    );
    assert_eq!(engine["engine_time_ns"]["value"], 750_000);
}

#[test]
fn no_gpu_procs_leaves_the_envelope_array_honestly_empty() {
    // When no GPU process is present the envelope keeps the honest empty
    // array wave-1 Track A already guaranteed — the wiring must not regress
    // it to an absent key or a fabricated row.
    let owned: Vec<OwnedProcessGpuEngines> = Vec::new();
    let entries = build_process_gpu_entries(&owned);
    let extras = ExportExtras {
        containers: &[],
        process_gpu_engines: &entries,
        suggested_thresholds: &[],
        hardware: None,
        npu_inventory: None,
    };
    let json = render_with_extras(extras);
    let value: serde_json::Value = serde_json::from_str(&json).expect("envelope JSON must parse");
    assert_eq!(
        value["process_gpu_engines"].as_array().map(Vec::len),
        Some(0),
        "no GPU procs must be an honest empty array, never absent or fabricated"
    );
}

#[test]
fn collect_bulk_with_no_processes_is_honestly_empty_on_every_os() {
    // An empty process list yields an honest empty on every platform — the
    // collector never invents a GPU proc to look busy.
    let collected = collect_bulk_process_gpu_engines(&[], 1_000);
    assert!(
        collected.is_empty(),
        "an empty process list must yield an honest empty, never a fabricated row"
    );
}

// ── Linux /proc fixture tests for the real bulk reader ──────────────────
#[cfg(target_os = "linux")]
fn fixture_root(label: &str) -> std::path::PathBuf {
    crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-cli-gpu-bulk-{label}-{}",
        std::process::id()
    ))
}

#[cfg(target_os = "linux")]
fn link_fd(proc_dir: &std::path::Path, fd: u32, target: &str) {
    use std::os::unix::fs::symlink;
    let fd_dir = proc_dir.join("fd");
    std::fs::create_dir_all(&fd_dir).expect("create fd dir");
    symlink(target, fd_dir.join(fd.to_string())).expect("symlink fd");
}

#[cfg(target_os = "linux")]
fn write_fdinfo(proc_dir: &std::path::Path, fd: u32, body: &str) {
    std::fs::create_dir_all(proc_dir.join("fdinfo")).expect("create fdinfo dir");
    std::fs::write(proc_dir.join("fdinfo").join(fd.to_string()), body)
        .expect("write fdinfo fixture");
}

#[cfg(target_os = "linux")]
#[test]
fn bulk_emits_only_gpu_procs_with_an_honest_cold_start_rate() {
    let root = fixture_root("mix");
    // pid 100: DRM render fd with two engine lines -> emitted.
    let gpu_proc = root.join("100");
    std::fs::create_dir_all(&gpu_proc).expect("create gpu proc dir");
    link_fd(&gpu_proc, 3, "/dev/dri/renderD128");
    link_fd(&gpu_proc, 4, "socket:[4242]");
    write_fdinfo(
        &gpu_proc,
        3,
        "drm-driver:\ti915\ndrm-engine-render:\t400000 ns\ndrm-engine-video:\t100000 ns\n",
    );
    // pid 200: only a non-DRM descriptor -> skipped (live non-GPU process).
    let plain_proc = root.join("200");
    std::fs::create_dir_all(plain_proc.join("fd")).expect("create plain fd dir");
    {
        use std::os::unix::fs::symlink;
        symlink("/var/log/app.log", plain_proc.join("fd").join("5")).expect("ln file");
    }
    // pid 300: no fd directory at all -> skipped (vanished pid).
    let _gone_proc = root.join("300");
    // pid 400: DRM fd whose fdinfo has no engine lines -> skipped.
    let quiet_proc = root.join("400");
    std::fs::create_dir_all(&quiet_proc).expect("create quiet proc dir");
    link_fd(&quiet_proc, 7, "/dev/dri/renderD129");
    write_fdinfo(
        &quiet_proc,
        7,
        "drm-driver:\ti915\ndrm-pdev:\t0000:03:00.0\n",
    );

    let now_ms = 9_000_u64;
    let breakdowns = collect_process_gpu_engines_bulk(&root, &[100, 200, 300, 400], now_ms, 1024);

    // Only the GPU process is emitted, never a fabricated row for the rest.
    let pids: Vec<u32> = breakdowns.iter().map(|(pid, _)| *pid).collect();
    assert_eq!(pids, vec![100]);

    let (_, engines) = &breakdowns[0];
    assert_eq!(engines.state, DeviceState::healthy(now_ms));
    // Engines are ordered by ascending name: render, then video.
    assert_eq!(engines.engines.len(), 2);
    assert_eq!(engines.engines[0].name, "render");
    assert_eq!(engines.engines[1].name, "video");
    // Single-tick cold start: every rate is the typed gap, never 0%.
    for engine in &engines.engines {
        assert!(
            engine.usage_pct.current_value().is_none(),
            "{} rate must be unavailable on the only tick, never a fabricated 0%",
            engine.name
        );
    }
    // The cumulative counters ARE observed this tick — honest readings.
    assert_eq!(
        engines.engines[0].engine_time_ns.current_value(),
        Some(&400_000_u64)
    );
    assert_eq!(
        engines.engines[1].engine_time_ns.current_value(),
        Some(&100_000_u64)
    );

    std::fs::remove_dir_all(&root).expect("cleanup bulk fixture");
}

#[cfg(target_os = "linux")]
#[test]
fn bulk_respects_the_process_cap_as_a_hard_safety_bound() {
    let root = fixture_root("cap");
    let now_ms = 1_000_u64;
    // Two GPU procs, but the cap is one: only the first pid in the input
    // order is scanned. A GPU process past the cap is an honest absence, not
    // a fabricated zero.
    let first = root.join("10");
    std::fs::create_dir_all(&first).expect("create first proc dir");
    link_fd(&first, 3, "/dev/dri/renderD128");
    write_fdinfo(&first, 3, "drm-engine-render:\t100 ns\n");
    let second = root.join("20");
    std::fs::create_dir_all(&second).expect("create second proc dir");
    link_fd(&second, 3, "/dev/dri/renderD129");
    write_fdinfo(&second, 3, "drm-engine-render:\t200 ns\n");

    let one = collect_process_gpu_engines_bulk(&root, &[10, 20], now_ms, 1);
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].0, 10);

    // Lifting the cap reaches both: the bound is a safety valve, not a fixed
    // limit.
    let both = collect_process_gpu_engines_bulk(&root, &[10, 20], now_ms, 1024);
    assert_eq!(both.len(), 2);

    std::fs::remove_dir_all(&root).expect("cleanup cap fixture");
}

#[cfg(target_os = "linux")]
#[test]
fn bulk_with_no_gpu_processes_is_an_honest_empty_not_a_failure() {
    // A root with only non-GPU processes yields an empty vec — the honest
    // representation of a host with no GPU clients, never an error and never
    // a fabricated row.
    let root = fixture_root("empty");
    let plain = root.join("7");
    std::fs::create_dir_all(plain.join("fd")).expect("create plain fd dir");
    let breakdowns = collect_process_gpu_engines_bulk(&root, &[7], 1_000, 1024);
    assert!(breakdowns.is_empty());
    std::fs::remove_dir_all(&root).expect("cleanup empty fixture");
}
