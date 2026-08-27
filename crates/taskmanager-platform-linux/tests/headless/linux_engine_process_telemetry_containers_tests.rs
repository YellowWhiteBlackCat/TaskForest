use super::*;
use crate::engine::process::telemetry::safe_cgroup_path;
use std::collections::HashSet;

fn fixture_tree(label: &str) -> PathBuf {
    // Reuse the agent lease tmp when present (mirrors the facet fixture
    // helpers); fall back to the platform temp dir otherwise.
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "taskmanager-container-rollup-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create fixture cgroup root");
    path
}

fn write_container(root: &Path, relative: &str, usage_usec: u64, mem_bytes: u64, pids: &[u32]) {
    let dir = safe_cgroup_path(root, relative).expect("safe container path");
    std::fs::create_dir_all(&dir).expect("create container cgroup dir");
    std::fs::write(
        dir.join("cpu.stat"),
        format!("usage_usec {usage_usec}\nuser_usec 0\n"),
    )
    .expect("write cpu.stat");
    std::fs::write(dir.join("memory.current"), format!("{mem_bytes}\n"))
        .expect("write memory.current");
    let procs = pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(dir.join("cgroup.procs"), format!("{procs}\n")).expect("write cgroup.procs");
}

#[test]
fn parse_cpu_stat_extracts_usage_usec_and_ignores_extra_keys() {
    let stat = "usage_usec 1234567\nuser_usec 1000000\nsystem_usec 234567\nnr_periods 4\n";
    assert_eq!(parse_cpu_stat_usage_usec(stat), Some(1_234_567));
    // Trailing tokens on the usage_usec line make it unparseable.
    assert_eq!(parse_cpu_stat_usage_usec("usage_usec 1 2\n"), None);
    assert_eq!(parse_cpu_stat_usage_usec("user_usec 9\n"), None);
    assert_eq!(parse_cpu_stat_usage_usec(""), None);
}

#[test]
fn parse_memory_current_trims_whitespace() {
    assert_eq!(parse_memory_current("  1048576\n"), Some(1_048_576));
    assert_eq!(parse_memory_current("not-a-number"), None);
}

#[test]
fn parse_cgroup_procs_skips_unparseable_lines() {
    let text = "100\n200\nbogus\n300\n";
    assert_eq!(parse_cgroup_procs(text), vec![100, 200, 300]);
}

#[test]
fn classify_recognises_runtime_signatures_and_rejects_plain_services() {
    assert_eq!(
        classify_container_cgroup("/docker/abc123def456"),
        Some(IsolationKind::Docker)
    );
    assert_eq!(
        classify_container_cgroup("/kubepods/besteffort/pod123"),
        Some(IsolationKind::Kubernetes)
    );
    assert_eq!(
        classify_container_cgroup("/libpod-abcdef.scope"),
        Some(IsolationKind::Podman)
    );
    assert_eq!(
        classify_container_cgroup("/machine.slice/machine-qemu.scope"),
        Some(IsolationKind::SystemdNspawn)
    );
    // A plain systemd service is NOT a container.
    assert_eq!(
        classify_container_cgroup("/system.slice/cron.service"),
        None
    );
    assert_eq!(
        classify_container_cgroup("/user.slice/user-1000.slice"),
        None
    );
}

#[test]
fn classify_excludes_runtime_engine_daemon_cgroups() {
    // The runtime engine's own systemd unit matches the substring
    // signature (`docker`, `containerd`, ...) but is the engine, not a
    // workload container — it must never be rolled up as a phantom.
    assert_eq!(
        classify_container_cgroup("/system.slice/docker.service"),
        None
    );
    assert_eq!(
        classify_container_cgroup("/system.slice/dockerd.service"),
        None
    );
    assert_eq!(
        classify_container_cgroup("/system.slice/containerd.service"),
        None
    );
    assert_eq!(
        classify_container_cgroup("/system.slice/podman.service"),
        None
    );
    assert_eq!(
        classify_container_cgroup("/system.slice/lxcfs.service"),
        None
    );
    // A real workload cgroup still classifies: the runtime's own daemon is
    // excluded, but a container placed under `/docker/<id>` is not.
    assert_eq!(
        classify_container_cgroup("/docker/0123456789abcdef0123456789abcdef"),
        Some(IsolationKind::Docker)
    );
    assert_eq!(
        classify_container_cgroup("/system.slice/docker-abcdef123456.scope"),
        Some(IsolationKind::Docker)
    );
}

#[test]
fn rate_first_sample_is_gap_and_second_sample_uses_elapsed() {
    let mut tracker = ContainerCpuRateTracker::default();
    let first = tracker.percentage("/docker/x", 1_000_000, 1_000);
    assert_eq!(
        first.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    // 2 cpu-seconds consumed over 1 second wall => 200%.
    let second = tracker.percentage("/docker/x", 3_000_000, 2_000);
    assert_eq!(second.current_value(), Some(&200.0));
}

#[test]
fn rate_counter_rollback_resets_baseline_to_identity_changed() {
    let mut tracker = ContainerCpuRateTracker::default();
    tracker.percentage("/docker/x", 5_000_000, 1_000);
    // usage_usec went backwards => the cgroup was recreated / counter
    // reset; the immediate response is typed IdentityChanged and the
    // baseline is re-seeded at the rolled-back reading.
    let rollback = tracker.percentage("/docker/x", 1_000, 2_000);
    assert_eq!(
        rollback.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::IdentityChanged)
    );
    // The next sample computes a fresh delta from the re-seeded baseline
    // (1_001_000 - 1_000 = 1 cpu-second over 1 s wall => 100%), mirroring
    // the established counter_delta recovery contract.
    let recovered = tracker.percentage("/docker/x", 1_001_000, 3_000);
    assert_eq!(recovered.current_value(), Some(&100.0));
}

#[test]
fn retain_paths_drops_destroyed_container_baselines() {
    let mut tracker = ContainerCpuRateTracker::default();
    tracker.percentage("/docker/a", 1_000, 1);
    tracker.percentage("/docker/b", 1_000, 1);
    let mut present = HashSet::new();
    present.insert("/docker/a".to_owned());
    tracker.retain_paths(&present);
    // Destroyed container b must not synthesize a stale delta.
    let ghost = tracker.percentage("/docker/b", 1_000, 2);
    assert!(ghost.current_value().is_none());
}

#[test]
fn collect_yields_unsupported_on_cgroup_v1_host() {
    let root = fixture_tree("v1");
    // No cgroup.controllers file => treated as a v1/unified-absent mount.
    let mut collector = ContainerRollupCollector::default();
    let rollup = collector.collect_from_root(&root, 1_000);
    assert_eq!(rollup.state.status, DeviceStatus::Unsupported);
    assert!(rollup.containers.is_empty());
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn collect_discovers_containers_and_aggregates_fields() {
    let root = fixture_tree("v2-populated");
    std::fs::write(root.join("cgroup.controllers"), "memory cpu\n").expect("v2 marker");
    write_container(
        &root,
        "/docker/0123456789abcdef0123456789abcdef",
        1_000_000,
        100 * 1024 * 1024,
        &[42, 43],
    );
    write_container(
        &root,
        "/libpod-abcdef123456.scope",
        1_000_000,
        50 * 1024 * 1024,
        &[77],
    );
    // A plain service directory must be ignored by discovery.
    let service = safe_cgroup_path(&root, "/system.slice/cron.service").unwrap();
    std::fs::create_dir_all(&service).expect("create service cgroup");
    std::fs::write(service.join("memory.current"), "999\n").ok();

    let mut collector = ContainerRollupCollector::default();
    // First pass: discovery finds both containers; CPU% is the gap sample.
    let first = collector.collect_from_root(&root, 1_000);
    assert_eq!(first.state.status, DeviceStatus::Healthy);
    assert_eq!(first.containers.len(), 2);
    assert!(
        first
            .containers
            .iter()
            .all(|c| c.cpu_percentage.current_value().is_none()),
        "first sample must be a typed gap, not a fabricated zero"
    );
    let docker = first
        .containers
        .iter()
        .find(|c| c.runtime == Some(IsolationKind::Docker))
        .expect("docker container discovered");
    assert_eq!(
        docker.memory_bytes.current_value(),
        Some(&(100 * 1024 * 1024))
    );
    assert_eq!(docker.member_pids, vec![42, 43]);

    // Second pass: cpu.stat advances for the docker container only.
    write_container(
        &root,
        "/docker/0123456789abcdef0123456789abcdef",
        3_000_000,
        120 * 1024 * 1024,
        &[42, 43],
    );
    let second = collector.collect_from_root(&root, 2_000);
    let docker = second
        .containers
        .iter()
        .find(|c| c.runtime == Some(IsolationKind::Docker))
        .expect("docker container still present");
    // 2 cpu-sec over 1 sec wall => 200% single-core-equivalent.
    assert_eq!(docker.cpu_percentage.current_value(), Some(&200.0));
    assert_eq!(
        docker.memory_bytes.current_value(),
        Some(&(120 * 1024 * 1024))
    );
    // Descending CPU% ordering: the busy container lands first.
    assert_eq!(second.containers[0].runtime, Some(IsolationKind::Docker));

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn collect_healthy_empty_when_no_container_cgroups_exist() {
    let root = fixture_tree("v2-empty");
    std::fs::write(root.join("cgroup.controllers"), "memory\n").expect("v2 marker");
    // Only a non-container service cgroup exists.
    let service = safe_cgroup_path(&root, "/system.slice/cron.service").unwrap();
    std::fs::create_dir_all(&service).expect("create service cgroup");

    let mut collector = ContainerRollupCollector::default();
    let rollup = collector.collect_from_root(&root, 5_000);
    assert_eq!(rollup.state.status, DeviceStatus::Healthy);
    assert!(rollup.containers.is_empty());
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn collect_typed_unavailable_for_vanished_cgroup_fields() {
    let root = fixture_tree("v2-vanish");
    std::fs::write(root.join("cgroup.controllers"), "memory cpu\n").expect("v2 marker");
    write_container(&root, "/docker/abcdef123456", 1_000_000, 1_024, &[]);
    // Sabotage cpu.stat after creation: the dir is still discovered but the
    // CPU field read fails (NotFound => IdentityChanged typed unavailable).
    let dir = safe_cgroup_path(&root, "/docker/abcdef123456").unwrap();
    std::fs::remove_file(dir.join("cpu.stat")).expect("remove cpu.stat");

    let mut collector = ContainerRollupCollector::default();
    let rollup = collector.collect_from_root(&root, 7_000);
    let container = rollup.containers.first().expect("container discovered");
    assert_eq!(
        container.cpu_percentage.availability(),
        taskmanager_core::ScalarAvailability::Unavailable(FailureKind::IdentityChanged)
    );
    // memory.current is still readable.
    assert_eq!(container.memory_bytes.current_value(), Some(&1_024));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn collect_marks_stale_when_discovery_cap_is_hit() {
    // Drive the capped branch with a tiny cap instead of materializing
    // 4096 fixture directories: the BFS must signal incompleteness and the
    // rollup must downgrade from Healthy to a typed Stale/partial state.
    let root = fixture_tree("v2-capped");
    std::fs::write(root.join("cgroup.controllers"), "memory cpu\n").expect("v2 marker");
    // More container cgroups than the small cap can visit.
    for index in 0..6u32 {
        let id = format!("{index:064x}");
        write_container(&root, &format!("/docker/{id}"), 1_000_000, 1_024, &[]);
    }
    let mut collector = ContainerRollupCollector::default();
    let rollup = collector.collect_from_root_bounded(&root, 1_000, 3);
    assert_ne!(
        rollup.state.status,
        DeviceStatus::Healthy,
        "a capped (incomplete) discovery must not overclaim Healthy"
    );
    assert_eq!(rollup.state.status, DeviceStatus::Stale);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn collect_marks_stale_when_cgroup_procs_is_unreadable() {
    // An unreadable `cgroup.procs` (here: removed, so NotFound) must not
    // collapse into a silent empty member list while the rollup still
    // reports Healthy. The container is still discovered (cpu.stat and
    // memory.current are readable); membership is honestly empty and the
    // rollup state downgrades to Stale/partial.
    let root = fixture_tree("v2-procs-fail");
    std::fs::write(root.join("cgroup.controllers"), "memory cpu\n").expect("v2 marker");
    write_container(&root, "/docker/abcdef123456", 1_000_000, 1_024, &[42]);
    let dir = safe_cgroup_path(&root, "/docker/abcdef123456").expect("safe container path");
    std::fs::remove_file(dir.join("cgroup.procs")).expect("remove cgroup.procs");

    let mut collector = ContainerRollupCollector::default();
    let rollup = collector.collect_from_root(&root, 7_000);
    let container = rollup
        .containers
        .first()
        .expect("container discovered from cpu.stat/memory.current");
    assert_eq!(container.memory_bytes.current_value(), Some(&1_024));
    assert!(
        container.member_pids.is_empty(),
        "failed membership stays empty (the model's unavailable form), not a fabricated list"
    );
    assert_ne!(
        rollup.state.status,
        DeviceStatus::Healthy,
        "a membership read failure must surface as a typed non-Healthy rollup"
    );
    assert_eq!(rollup.state.status, DeviceStatus::Stale);
    std::fs::remove_dir_all(root).ok();
}
