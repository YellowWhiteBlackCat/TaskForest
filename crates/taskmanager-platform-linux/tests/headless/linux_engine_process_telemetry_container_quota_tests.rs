//! test-intent: behavior
//!
//! The container quota collector reads the cgroup-v2 limit files of the
//! container it rolls up. The tests below drive a fixture cgroup tree (never
//! the host's real cgroups) and pin the three honest outcomes: a declared
//! ceiling, a dimension that declares nothing, and a limit file that could
//! not be read.

use super::*;
use crate::engine::process::telemetry::safe_cgroup_path;
use taskmanager_core::ContainerCpuQuota;

fn quota_fixture_tree(label: &str) -> PathBuf {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "taskmanager-container-quota-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create fixture cgroup root");
    path
}

fn write_container(root: &Path, relative: &str) -> PathBuf {
    let dir = safe_cgroup_path(root, relative).expect("safe container path");
    std::fs::create_dir_all(&dir).expect("create container cgroup dir");
    std::fs::write(dir.join("cpu.stat"), "usage_usec 1000000\n").expect("write cpu.stat");
    std::fs::write(dir.join("memory.current"), "1048576\n").expect("write memory.current");
    std::fs::write(dir.join("cgroup.procs"), "42\n").expect("write cgroup.procs");
    dir
}

#[test]
fn a_memory_limit_payload_distinguishes_a_ceiling_from_no_ceiling() {
    assert_eq!(
        parse_memory_limit("536870912\n"),
        Some(MemoryLimitDeclaration::Ceiling(536_870_912))
    );
    assert_eq!(
        parse_memory_limit("max\n"),
        Some(MemoryLimitDeclaration::NoCeiling)
    );
    assert_eq!(parse_memory_limit("not-a-number"), None);
    assert_eq!(parse_memory_limit(""), None);
}

#[test]
fn a_cpu_max_payload_carries_the_budget_and_its_period() {
    let quota = parse_cpu_max("200000 100000\n").expect("a cpu.max document");
    assert_eq!(quota.quota_us, Some(200_000));
    assert_eq!(quota.period_us, 100_000);

    // `max` names no budget but still declares the measurement window.
    let unlimited = parse_cpu_max("max 100000\n").expect("a cpu.max document");
    assert_eq!(unlimited.quota_us, None);
    assert_eq!(unlimited.period_us, 100_000);

    assert_eq!(parse_cpu_max("200000"), None);
    assert_eq!(parse_cpu_max("200000 100000 extra"), None);
}

#[test]
fn a_container_without_limit_files_declares_no_ceiling() {
    let root = quota_fixture_tree("no-limits");
    std::fs::write(root.join("cgroup.controllers"), "memory cpu\n").expect("v2 marker");
    write_container(&root, "/docker/abcdef123456");

    let mut collector = ContainerRollupCollector::default();
    let rollup = collector.collect_from_root(&root, 1_000);
    let container = rollup.containers.first().expect("container discovered");

    assert_eq!(container.quota.memory_hard_limit_bytes, None);
    assert_eq!(container.quota.memory_soft_limit_bytes, None);
    assert_eq!(container.quota.cpu, None);
    assert_eq!(container.quota.failure, None);
    assert!(!container.quota.declares_any_ceiling());

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_container_publishes_the_ceilings_its_cgroup_declares() {
    let root = quota_fixture_tree("declared");
    std::fs::write(root.join("cgroup.controllers"), "memory cpu\n").expect("v2 marker");
    let dir = write_container(&root, "/docker/abcdef123456");
    std::fs::write(dir.join("memory.max"), "2147483648\n").expect("write memory.max");
    std::fs::write(dir.join("memory.high"), "1073741824\n").expect("write memory.high");
    std::fs::write(dir.join("cpu.max"), "200000 100000\n").expect("write cpu.max");

    let mut collector = ContainerRollupCollector::default();
    let rollup = collector.collect_from_root(&root, 1_000);
    let container = rollup.containers.first().expect("container discovered");

    assert_eq!(container.quota.memory_hard_limit_bytes, Some(2_147_483_648));
    assert_eq!(container.quota.memory_soft_limit_bytes, Some(1_073_741_824));
    assert_eq!(
        container.quota.cpu,
        Some(ContainerCpuQuota {
            quota_us: Some(200_000),
            period_us: 100_000,
        })
    );
    assert_eq!(container.quota.failure, None);
    assert!(container.quota.declares_any_ceiling());

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_max_payload_declares_no_ceiling_in_that_dimension() {
    let root = quota_fixture_tree("max-payload");
    std::fs::write(root.join("cgroup.controllers"), "memory cpu\n").expect("v2 marker");
    let dir = write_container(&root, "/docker/abcdef123456");
    std::fs::write(dir.join("memory.max"), "max\n").expect("write memory.max");
    std::fs::write(dir.join("memory.high"), "1073741824\n").expect("write memory.high");

    let mut collector = ContainerRollupCollector::default();
    let rollup = collector.collect_from_root(&root, 1_000);
    let container = rollup.containers.first().expect("container discovered");

    assert_eq!(container.quota.memory_hard_limit_bytes, None);
    assert_eq!(container.quota.memory_soft_limit_bytes, Some(1_073_741_824));
    assert_eq!(container.quota.failure, None);

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_malformed_limit_file_makes_the_quota_unknown_instead_of_guessed() {
    let root = quota_fixture_tree("malformed");
    std::fs::write(root.join("cgroup.controllers"), "memory cpu\n").expect("v2 marker");
    let dir = write_container(&root, "/docker/abcdef123456");
    std::fs::write(dir.join("memory.max"), "2147483648\n").expect("write memory.max");
    std::fs::write(dir.join("memory.high"), "not-a-number\n").expect("write memory.high");

    let mut collector = ContainerRollupCollector::default();
    let rollup = collector.collect_from_root(&root, 1_000);
    let container = rollup.containers.first().expect("container discovered");

    assert_eq!(
        container.quota.failure,
        Some(FailureKind::ProviderFault),
        "an unreadable declaration must be a typed failure"
    );
    assert_eq!(
        container.quota.memory_hard_limit_bytes, None,
        "a partially-read quota must not publish a ceiling it did not read"
    );
    assert_eq!(container.quota.memory_soft_limit_bytes, None);
    assert!(!container.quota.declares_any_ceiling());

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn the_declared_soft_ceiling_drives_the_over_limit_rule() {
    let root = quota_fixture_tree("over-limit");
    std::fs::write(root.join("cgroup.controllers"), "memory cpu\n").expect("v2 marker");
    let dir = write_container(&root, "/docker/abcdef123456");
    std::fs::write(dir.join("memory.high"), "1048576\n").expect("write memory.high");
    std::fs::write(dir.join("memory.current"), "2097152\n").expect("write memory.current");

    let mut collector = ContainerRollupCollector::default();
    let rollup = collector.collect_from_root(&root, 1_000);
    let container = rollup.containers.first().expect("container discovered");

    assert_eq!(
        container
            .quota
            .soft_memory_usage_percent(container.memory_bytes.current_value().copied()),
        Some(200.0)
    );

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn the_payload_shapes_a_live_cgroup_writes_are_read_as_declared_or_undeclared() {
    // The exact payload shapes the kernel writes into a cgroup on a live
    // host: an unlimited memory dimension, a byte ceiling, and a CPU quota
    // that names no budget but still declares its period.
    assert_eq!(
        parse_memory_limit("max\n"),
        Some(MemoryLimitDeclaration::NoCeiling)
    );
    assert_eq!(
        parse_memory_limit("67108864\n"),
        Some(MemoryLimitDeclaration::Ceiling(67_108_864))
    );

    let cpu = parse_cpu_max("max 100000\n").expect("a cpu.max document");
    assert_eq!(cpu.quota_us, None);
    assert_eq!(cpu.period_us, 100_000);
}
