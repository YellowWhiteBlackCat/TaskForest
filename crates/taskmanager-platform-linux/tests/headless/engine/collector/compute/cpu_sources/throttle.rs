use super::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use taskmanager_core::CpuThrottlePackageCounters;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct FixtureDir(PathBuf);

impl FixtureDir {
    fn new() -> Self {
        let path = crate::test_support::repo_temp_dir().join(format!(
            "taskmanager-cpu-throttle-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("create CPU throttle fixture");
        Self(path)
    }

    fn cpu_root(&self) -> PathBuf {
        self.0.join("cpu")
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Write one logical CPU's topology identity and optional counter files.
fn write_cpu(
    cpu_root: &Path,
    cpu: u32,
    package_id: &str,
    core_id: Option<&str>,
    core_count: Option<&str>,
    package_count: Option<&str>,
) {
    let topology = cpu_root.join(format!("cpu{cpu}/topology"));
    std::fs::create_dir_all(&topology).expect("topology fixture");
    std::fs::write(
        topology.join("physical_package_id"),
        format!("{package_id}\n"),
    )
    .expect("package identity");
    if let Some(core_id) = core_id {
        std::fs::write(topology.join("core_id"), format!("{core_id}\n")).expect("core identity");
    }
    let throttle = cpu_root.join(format!("cpu{cpu}/thermal_throttle"));
    if core_count.is_none() && package_count.is_none() {
        return;
    }
    std::fs::create_dir_all(&throttle).expect("throttle fixture");
    if let Some(counter) = core_count {
        std::fs::write(throttle.join("core_throttle_count"), format!("{counter}\n"))
            .expect("core counter");
    }
    if let Some(counter) = package_count {
        std::fs::write(
            throttle.join("package_throttle_count"),
            format!("{counter}\n"),
        )
        .expect("package counter");
    }
}

#[test]
fn siblings_are_de_duplicated_and_packages_keep_their_own_counters() {
    let fixture = FixtureDir::new();
    let cpu_root = fixture.cpu_root();
    write_cpu(&cpu_root, 0, "0", Some("0"), Some("3"), Some("7"));
    write_cpu(&cpu_root, 1, "0", Some("0"), Some("3"), Some("7"));
    write_cpu(&cpu_root, 2, "0", Some("1"), Some("4"), Some("7"));
    write_cpu(&cpu_root, 3, "1", Some("0"), Some("1"), Some("2"));

    let snapshot = collect_package_counters_at(&cpu_root);

    assert!(snapshot.is_success());
    assert_eq!(
        snapshot.packages,
        vec![
            CpuThrottlePackageCounters {
                package_id: 0,
                package_throttle_count: Some(7),
                core_throttle_count: Some(7),
            },
            CpuThrottlePackageCounters {
                package_id: 1,
                package_throttle_count: Some(2),
                core_throttle_count: Some(1),
            },
        ]
    );
}

#[test]
fn a_counter_no_read_could_observe_stays_absent() {
    let fixture = FixtureDir::new();
    let cpu_root = fixture.cpu_root();
    write_cpu(&cpu_root, 0, "0", Some("0"), None, None);
    write_cpu(&cpu_root, 1, "1", Some("0"), None, Some("5"));

    let snapshot = collect_package_counters_at(&cpu_root);

    assert!(snapshot.is_success());
    assert_eq!(snapshot.packages[0].package_id, 0);
    assert_eq!(snapshot.packages[0].package_throttle_count, None);
    assert_eq!(snapshot.packages[0].core_throttle_count, None);
    assert_eq!(snapshot.packages[1].package_throttle_count, Some(5));
    assert_eq!(snapshot.packages[1].core_throttle_count, None);
}

#[test]
fn a_cpu_without_core_identity_contributes_only_its_package_counter() {
    let fixture = FixtureDir::new();
    let cpu_root = fixture.cpu_root();
    write_cpu(&cpu_root, 0, "0", None, Some("9"), Some("4"));

    let snapshot = collect_package_counters_at(&cpu_root);

    assert_eq!(
        snapshot.packages,
        vec![CpuThrottlePackageCounters {
            package_id: 0,
            package_throttle_count: Some(4),
            core_throttle_count: None,
        }]
    );
}

#[test]
fn an_unreadable_cpu_root_is_a_typed_failure() {
    let fixture = FixtureDir::new();
    let missing = fixture.0.join("no-cpu-root");

    let snapshot = collect_package_counters_at(&missing);

    assert!(!snapshot.is_success());
    assert!(snapshot.packages.is_empty());
    assert_eq!(
        snapshot.failure.as_ref().map(|failure| failure.kind),
        Some(FailureKind::Unsupported)
    );
    assert_eq!(
        io_failure(&std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        FailureKind::PermissionDenied
    );
}

#[test]
fn a_readable_host_without_throttle_files_is_a_successful_absence() {
    let fixture = FixtureDir::new();
    let cpu_root = fixture.cpu_root();
    write_cpu(&cpu_root, 0, "0", Some("0"), None, None);

    let snapshot = collect_package_counters_at(&cpu_root);

    assert!(
        snapshot.is_success(),
        "a host that exposes no counter is not a read failure"
    );
    assert_eq!(snapshot.packages.len(), 1);
}

/// The two carriers of the counter fact must be same-origin: the periodic CPU
/// package observation and the `telemetry.cpu.throttle` lane snapshot read one
/// implementation and one aggregation, so an SMT de-duplication, a missing
/// counter, or a multi-package host cannot make them disagree.
#[test]
fn the_package_observation_and_the_lane_snapshot_share_one_aggregation() {
    let fixture = FixtureDir::new();
    let cpu_root = fixture.cpu_root();
    write_cpu(&cpu_root, 0, "0", Some("0"), Some("3"), Some("7"));
    write_cpu(&cpu_root, 1, "0", Some("0"), Some("3"), Some("7"));
    write_cpu(&cpu_root, 2, "0", Some("1"), Some("4"), Some("7"));
    write_cpu(&cpu_root, 3, "1", Some("0"), None, Some("2"));

    let lane = collect_package_counters_at(&cpu_root);
    let packages =
        super::super::diagnostics::observe_cpu_packages_at(&cpu_root, &fixture.0.join("node"), 4);

    assert_eq!(lane.packages.len(), packages.len());
    for (row, package) in lane.packages.iter().zip(&packages) {
        assert_eq!(package.package_id, row.package_id);
        assert_eq!(package.package_throttle_count, row.package_throttle_count);
        assert_eq!(package.core_throttle_count, row.core_throttle_count);
    }
}
