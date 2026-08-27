use super::*;
use crate::engine::process::telemetry::state_for_status;
use taskmanager_core::core::device_state::DeviceState;

const FDINFO_RENDER_VIDEO: &str = "\
pos:\t0
flags:\t0100002
mnt_ref:\t1
drm-driver:\ti915
drm-pdev:\t0000:03:00.0
drm-client-id:\t7
drm-engine-render:\t500000 ns
drm-engine-copy:\t0 ns
drm-engine-video:\t1250000 ns
";

const FDINFO_NO_ENGINES: &str = "\
pos:\t0
drm-driver:\ti915
drm-pdev:\t0000:03:00.0
drm-client-id:\t7
";

#[test]
fn parse_sums_per_engine_and_ignores_non_engine_lines() {
    let engines = parse_drm_engine_counters(FDINFO_RENDER_VIDEO);
    assert_eq!(engines.get("render").and_then(|c| c.ns), Some(500_000));
    assert_eq!(engines.get("video").and_then(|c| c.ns), Some(1_250_000));
    assert_eq!(engines.get("copy").and_then(|c| c.ns), Some(0));
    assert!(!engines.contains_key("drm-pdev"));
    assert_eq!(engines.len(), 3);
}

#[test]
fn parse_handles_required_ns_unit_and_rejects_non_time_values() {
    // Canonical `ns` suffix parses.
    assert_eq!(parse_engine_ns("500 ns"), Some(500));
    // A bare number is rejected: without the unit a non-time value could
    // pose as busy nanoseconds, so the honest answer is "not a reading".
    assert_eq!(parse_engine_ns("500"), None);
    // A mismatched unit is rejected so a non-time value cannot pose as ns.
    assert_eq!(parse_engine_ns("500 bytes"), None);
    assert_eq!(parse_engine_ns("notanumber ns"), None);
    assert_eq!(parse_engine_ns(""), None);
}

#[test]
fn parse_handles_empty_and_non_drm_text() {
    assert!(parse_drm_engine_counters("").is_empty());
    assert!(parse_drm_engine_counters("no drm keys here\nposix").is_empty());
}

/// xe-driver fdinfo (Panther Lake / B-series, kernel exposing cumulative
/// cycles instead of i915-style busy ns).
const FDINFO_XE_CYCLES: &str = "\
drm-driver:\txe
drm-client-id:\t20
drm-pdev:\t0000:00:02.0
drm-total-system:\t17204 KiB
drm-total-gtt:\t176 KiB
drm-total-cycles-rcs:\t643228675411
drm-total-cycles-vcs:\t643228675411
drm-total-cycles-vecs:\t643228675411
drm-total-cycles-bcs:\t643228675411
drm-total-cycles-ccs:\t643228675411
";

/// xe fdinfo on kernels that additionally expose busy nanoseconds.
const FDINFO_XE_BUSY: &str = "\
drm-driver:\txe
drm-client-id:\t7
drm-total-busy-rcs:\t750000000 ns
drm-total-busy-vcs:\t1250000 ns
";

#[test]
fn parse_xe_cycles_abi_without_ns_unit_requirement() {
    let engines = parse_drm_engine_counters(FDINFO_XE_CYCLES);
    assert_eq!(engines.len(), 5);
    assert_eq!(
        engines.get("rcs").and_then(|c| c.cycles),
        Some(643_228_675_411)
    );
    assert!(
        engines.get("rcs").and_then(|c| c.ns).is_none(),
        "a cycles-only source must not fabricate busy nanoseconds"
    );
    assert!(
        !engines.contains_key("drm-total-system"),
        "memory lines are not engines"
    );
}

#[test]
fn parse_xe_busy_ns_abi_maps_onto_the_ns_counter() {
    let engines = parse_drm_engine_counters(FDINFO_XE_BUSY);
    assert_eq!(engines.get("rcs").and_then(|c| c.ns), Some(750_000_000));
    assert_eq!(engines.get("vcs").and_then(|c| c.ns), Some(1_250_000));
    assert!(engines.get("rcs").and_then(|c| c.cycles).is_none());
}

#[test]
fn parse_merges_busy_and_cycles_into_one_engine_entry() {
    let text = "drm-total-busy-rcs:\t100 ns\ndrm-total-cycles-rcs:\t42\n";
    let engines = parse_drm_engine_counters(text);
    assert_eq!(engines.len(), 1);
    assert_eq!(engines.get("rcs").and_then(|c| c.ns), Some(100));
    assert_eq!(engines.get("rcs").and_then(|c| c.cycles), Some(42));
}

#[test]
fn is_drm_render_target_matches_render_card_and_by_path() {
    assert!(is_drm_render_target("/dev/dri/renderD128"));
    assert!(is_drm_render_target("/dev/dri/card0"));
    assert!(is_drm_render_target(
        "/dev/dri/by-path/pci-0000:03:00.0-render"
    ));
    // Sockets, pipes, and unrelated device files are not DRM descriptors.
    assert!(!is_drm_render_target("socket:[4242]"));
    assert!(!is_drm_render_target("/dev/null"));
    assert!(!is_drm_render_target("/run/taskmanager.sock"));
}

fn identity(pid: u32, start: u64) -> ProcessIdentity {
    ProcessIdentity {
        pid,
        start_token: start,
    }
}

#[test]
fn rate_needs_two_samples_then_converts_and_resets_on_rollback() {
    let id = identity(9, 10);
    let raw = |ns| RawGpuEngineSnapshot {
        state: DeviceState::healthy(1),
        engines: vec![(
            "render".into(),
            RawEngineCounters {
                ns: Some(ns),
                ..Default::default()
            },
        )],
    };
    let mut tracker = ProcessGpuEngineRateTracker::default();

    // First sample: typed gap — no rate yet, but the cumulative is observed.
    let first = tracker.observe(id, 1_000, raw(0));
    assert_eq!(first.engines.len(), 1);
    assert_eq!(first.engines[0].name, "render");
    assert!(first.engines[0].usage_pct.current_value().is_none());
    assert_eq!(
        first.engines[0].engine_time_ns.current_value(),
        Some(&0_u64)
    );

    // +500 ms busy over +1 s wall clock → 50% single-core-equivalent.
    let second = tracker.observe(id, 2_000, raw(500_000_000));
    let rate = second.engines[0]
        .usage_pct
        .current_value()
        .copied()
        .unwrap_or(f32::NAN);
    assert!((rate - 50.0).abs() < 0.01, "expected ~50%, got {rate}");

    // Counter rollback → baseline reset, typed IdentityChanged, no rate.
    let reset = tracker.observe(id, 3_000, raw(100));
    assert!(reset.engines[0].usage_pct.current_value().is_none());

    // After reseed, a forward step produces a clean rate again.
    let resumed = tracker.observe(id, 4_000, raw(500_000_100));
    let resumed_rate = resumed.engines[0]
        .usage_pct
        .current_value()
        .copied()
        .unwrap_or(f32::NAN);
    assert!(
        (resumed_rate - 50.0).abs() < 0.01,
        "expected ~50% after reseed, got {resumed_rate}"
    );
}

#[test]
fn rate_is_single_core_equivalent_and_clamps_to_100() {
    let id = identity(1, 1);
    let raw = |ns| RawGpuEngineSnapshot {
        state: DeviceState::healthy(1),
        engines: vec![(
            "render".into(),
            RawEngineCounters {
                ns: Some(ns),
                ..Default::default()
            },
        )],
    };
    let mut tracker = ProcessGpuEngineRateTracker::default();
    let seed = tracker.observe(id, 1_000, raw(0));
    assert!(seed.engines[0].usage_pct.current_value().is_none());
    // +2 s busy over +1 s wall clock would naïvely be 200%; it must clamp.
    let clamped = tracker.observe(id, 2_000, raw(2_000_000_000));
    let rate = clamped.engines[0]
        .usage_pct
        .current_value()
        .copied()
        .unwrap_or(f32::NAN);
    assert!(
        (rate - 100.0).abs() < 0.01,
        "expected clamped 100%, got {rate}"
    );
}

#[test]
fn cycles_only_source_keeps_typed_gap_and_exposes_cycle_counts() {
    let id = identity(3, 1);
    let raw_cycles = |cycles| RawGpuEngineSnapshot {
        state: DeviceState::healthy(1),
        engines: vec![(
            "vcs".into(),
            RawEngineCounters {
                cycles: Some(cycles),
                ..Default::default()
            },
        )],
    };
    let mut tracker = ProcessGpuEngineRateTracker::default();

    // Both ticks: no busy-ns source → rate stays a typed gap (cycles alone
    // cannot be converted without the GT clock), the cycle count is the
    // honest observable, and the ns counter is never fabricated.
    let first = tracker.observe(id, 1_000, raw_cycles(643_228_675_411));
    assert_eq!(first.engines[0].name, "vcs");
    assert!(
        first.engines[0].usage_pct.current_value().is_none(),
        "cycles-only must not fabricate a percentage"
    );
    assert_eq!(
        first.engines[0].engine_cycles.current_value(),
        Some(&643_228_675_411_u64)
    );
    assert!(first.engines[0].engine_time_ns.current_value().is_none());

    let second = tracker.observe(id, 2_000, raw_cycles(643_228_675_999));
    assert!(second.engines[0].usage_pct.current_value().is_none());
    assert_eq!(
        second.engines[0].engine_cycles.current_value(),
        Some(&643_228_675_999_u64)
    );
}

#[test]
fn rate_state_is_scoped_to_raw_process_identity() {
    let raw = |ns| RawGpuEngineSnapshot {
        state: DeviceState::healthy(1),
        engines: vec![(
            "render".into(),
            RawEngineCounters {
                ns: Some(ns),
                ..Default::default()
            },
        )],
    };
    let mut tracker = ProcessGpuEngineRateTracker::default();
    // First identity seeds; second tick produces a rate.
    let a = identity(42, 100);
    let seed = tracker.observe(a, 1_000, raw(0));
    assert!(seed.engines[0].usage_pct.current_value().is_none());
    let with_rate = tracker.observe(a, 2_000, raw(500_000_000));
    assert!(with_rate.engines[0].usage_pct.current_value().is_some());

    // PID reuse (different start token) must not inherit the prior rate.
    let reused = identity(42, 200);
    let reused_snapshot = tracker.observe(reused, 3_000, raw(500_000_000));
    assert!(
        reused_snapshot.engines[0]
            .usage_pct
            .current_value()
            .is_none()
    );
}

/// The live-pid prune contract: baselines for pids that left the authoritative
/// set are dropped (a re-observation re-seeds with the typed first-sighting
/// gap), while live pids — including other open insight targets — keep theirs.
#[test]
fn rate_tracker_prunes_exited_pids_against_the_live_set() {
    let raw = |ns| RawGpuEngineSnapshot {
        state: DeviceState::healthy(1),
        engines: vec![(
            "render".into(),
            RawEngineCounters {
                ns: Some(ns),
                ..Default::default()
            },
        )],
    };
    let live = identity(7, 70);
    let exited = identity(8, 80);
    let mut tracker = ProcessGpuEngineRateTracker::default();
    let _ = tracker.observe(live, 1_000, raw(0));
    let _ = tracker.observe(exited, 1_000, raw(0));

    tracker.retain_live_pids(&HashSet::from([7]));

    let kept = tracker.observe(live, 2_000, raw(500_000_000));
    let kept_rate = kept.engines[0]
        .usage_pct
        .current_value()
        .copied()
        .unwrap_or(f32::NAN);
    assert!(
        (kept_rate - 50.0).abs() < 0.01,
        "live target keeps its rate"
    );
    let reseeded = tracker.observe(exited, 2_000, raw(500_000_000));
    assert!(
        reseeded.engines[0].usage_pct.current_value().is_none(),
        "a pruned pid must re-seed, not rate-convert off its dead baseline"
    );
    assert_eq!(
        reseeded.engines[0].usage_pct.availability().failure(),
        Some(FailureKind::TemporarilyUnavailable)
    );
}

#[test]
fn observe_preserves_engine_order_and_unhealthy_state() {
    let raw = RawGpuEngineSnapshot {
        state: state_for_status(DeviceStatus::PermissionDenied, 7_000),
        engines: Vec::new(),
    };
    let mut tracker = ProcessGpuEngineRateTracker::default();
    let breakdown = tracker.observe(identity(1, 1), 7_000, raw);
    assert_eq!(breakdown.state.status, DeviceStatus::PermissionDenied);
    assert!(breakdown.engines.is_empty());
}

#[cfg(target_os = "linux")]
fn fixture_root(label: &str) -> std::path::PathBuf {
    crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-gpu-engines-{label}-{}",
        std::process::id()
    ))
}

#[cfg(target_os = "linux")]
fn write_fdinfo(proc_dir: &std::path::Path, fd: u32, body: &str) {
    std::fs::create_dir_all(proc_dir.join("fdinfo")).expect("create fdinfo dir");
    std::fs::write(proc_dir.join("fdinfo").join(fd.to_string()), body)
        .expect("write fdinfo fixture");
}

#[cfg(target_os = "linux")]
fn link_fd(proc_dir: &std::path::Path, fd: u32, target: &str) {
    use std::os::unix::fs::symlink;
    let fd_dir = proc_dir.join("fd");
    std::fs::create_dir_all(&fd_dir).expect("create fd dir");
    symlink(target, fd_dir.join(fd.to_string())).expect("symlink drm fd");
}

#[cfg(target_os = "linux")]
#[test]
fn collect_aggregates_engines_across_drm_fds_and_skips_non_drm() {
    let root = fixture_root("aggregate");
    let proc_dir = root.join("42");
    std::fs::create_dir_all(&proc_dir).expect("create proc dir");
    // fd 3 and fd 7 are both render nodes; fd 4 is a socket (must be
    // ignored), fd 9 is /dev/null (ignored).
    link_fd(&proc_dir, 3, "/dev/dri/renderD128");
    link_fd(&proc_dir, 7, "/dev/dri/renderD129");
    link_fd(&proc_dir, 4, "socket:[4242]");
    link_fd(&proc_dir, 9, "/dev/null");
    write_fdinfo(
        &proc_dir,
        3,
        "drm-driver:\ti915\ndrm-engine-render:\t400000 ns\ndrm-engine-video:\t100000 ns\n",
    );
    write_fdinfo(
        &proc_dir,
        7,
        "drm-driver:\ti915\ndrm-engine-render:\t100000 ns\n",
    );

    let snapshot = collect_gpu_engines_from_proc_dir(&proc_dir, 1_000);
    assert_eq!(snapshot.state, DeviceState::healthy(1_000));
    // render summed across both DRM fds: 400000 + 100000 == 500000.
    let render_ns = snapshot
        .engines
        .iter()
        .find(|(name, _)| name == "render")
        .and_then(|(_, counters)| counters.ns)
        .expect("render engine aggregated across DRM fds");
    assert_eq!(render_ns, 500_000);
    let video_ns = snapshot
        .engines
        .iter()
        .find(|(name, _)| name == "video")
        .and_then(|(_, counters)| counters.ns)
        .expect("video engine present from the first DRM fd");
    assert_eq!(video_ns, 100_000);

    std::fs::remove_dir_all(&root).expect("cleanup fixture");
}

#[cfg(target_os = "linux")]
#[test]
fn collect_non_gpu_process_is_a_healthy_empty() {
    let root = fixture_root("non-gpu");
    let proc_dir = root.join("42");
    std::fs::create_dir_all(proc_dir.join("fd")).expect("create fd dir");
    // Only non-DRM descriptors — the honest result is a healthy empty list.
    {
        use std::os::unix::fs::symlink;
        symlink("/var/log/app.log", proc_dir.join("fd").join("3")).expect("ln file");
    }

    let snapshot = collect_gpu_engines_from_proc_dir(&proc_dir, 1_000);
    assert_eq!(snapshot.state, DeviceState::healthy(1_000));
    assert!(snapshot.engines.is_empty());

    std::fs::remove_dir_all(&root).expect("cleanup fixture");
}

#[cfg(target_os = "linux")]
#[test]
fn collect_drm_fd_without_engine_lines_is_a_healthy_empty() {
    let root = fixture_root("no-engines");
    let proc_dir = root.join("42");
    std::fs::create_dir_all(&proc_dir).expect("create proc dir");
    link_fd(&proc_dir, 5, "/dev/dri/renderD128");
    write_fdinfo(&proc_dir, 5, FDINFO_NO_ENGINES);

    let snapshot = collect_gpu_engines_from_proc_dir(&proc_dir, 1_000);
    assert_eq!(snapshot.state, DeviceState::healthy(1_000));
    assert!(snapshot.engines.is_empty());

    std::fs::remove_dir_all(&root).expect("cleanup fixture");
}

#[cfg(target_os = "linux")]
#[test]
fn collect_vanished_pid_is_stale_and_permission_denied_fdinfo_is_typed() {
    let root = fixture_root("typed-paths");
    let proc_dir = root.join("42");

    // No fd directory at all → Stale (vanished pid).
    let stale = collect_gpu_engines_from_proc_dir(&proc_dir, 1_000);
    assert_eq!(stale.state.status, DeviceStatus::Stale);
    assert!(stale.engines.is_empty());

    // A DRM fd whose fdinfo is absent (race) is skipped, yielding a healthy
    // empty rather than a failure — the descriptor was classifiable but no
    // engine lines were readable.
    std::fs::create_dir_all(&proc_dir).expect("create proc dir");
    link_fd(&proc_dir, 6, "/dev/dri/renderD128");
    let healthy_empty = collect_gpu_engines_from_proc_dir(&proc_dir, 2_000);
    assert_eq!(healthy_empty.state, DeviceState::healthy(2_000));
    assert!(healthy_empty.engines.is_empty());

    std::fs::remove_dir_all(&root).expect("cleanup fixture");
}
