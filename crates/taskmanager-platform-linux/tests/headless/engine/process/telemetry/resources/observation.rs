use std::sync::atomic::{AtomicU64, Ordering};

use taskmanager_core::{LimitValue, ResourceLastObservation};

use super::*;

impl ProcessResourceTracker {
    fn cached_identity(&self) -> Option<ProcessIdentity> {
        self.previous.as_ref().map(|(identity, _)| *identity)
    }
}

const LIMITS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/proc_limits.txt"
));
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    proc_dir: PathBuf,
    cgroup_root: PathBuf,
}

impl Fixture {
    fn new(start_token: u64, membership: &str) -> Self {
        let suffix = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = crate::test_support::repo_temp_dir().join(format!(
            "taskmanager-resource-observation-{}-{suffix}",
            std::process::id()
        ));
        let proc_dir = root.join("proc");
        let cgroup_root = root.join("cgroup");
        fs::create_dir_all(&proc_dir).unwrap();
        fs::create_dir_all(&cgroup_root).unwrap();
        fs::write(proc_dir.join("limits"), LIMITS).unwrap();
        fs::write(proc_dir.join("cgroup"), membership).unwrap();
        fs::write(proc_dir.join("stat"), stat_text(start_token)).unwrap();
        Self {
            root,
            proc_dir,
            cgroup_root,
        }
    }

    fn write_v2(&self, group: &str, memory_current: &str) {
        let dir = safe_cgroup_path(&self.cgroup_root, group).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("memory.current"), memory_current).unwrap();
        fs::write(dir.join("memory.max"), "max\n").unwrap();
        fs::write(dir.join("cpu.max"), "0 100000\n").unwrap();
        fs::write(dir.join("pids.current"), "0\n").unwrap();
        fs::write(dir.join("pids.max"), "max\n").unwrap();
    }

    fn identity(&self, start_token: u64) -> ProcessIdentity {
        ProcessIdentity {
            pid: 42,
            start_token,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).ok();
    }
}

fn stat_text(start_token: u64) -> String {
    let mut fields = vec!["S".to_owned()];
    fields.extend((0..18).map(|_| "0".to_owned()));
    fields.push(start_token.to_string());
    format!("42 (fixture worker) {}\n", fields.join(" "))
}

#[test]
fn v2_zero_unlimited_and_source_truth_are_current() {
    let fixture = Fixture::new(900, "0::/app.scope\n");
    fixture.write_v2("/app.scope", "0\n");

    let snapshot = collect_from_roots(&fixture.proc_dir, &fixture.cgroup_root, 10);

    assert_eq!(snapshot.current_memory_usage_bytes(), Some(0));
    assert_eq!(snapshot.current_memory_limit(), Some(LimitValue::Unlimited));
    assert_eq!(
        snapshot.current_cpu_time_quota_micros(),
        Some(LimitValue::Value(0))
    );
    assert_eq!(snapshot.current_cpu_time_period_micros(), Some(100_000));
    assert_eq!(snapshot.current_process_count(), Some(0));
    assert_eq!(
        snapshot.current_process_limit(),
        Some(LimitValue::Unlimited)
    );
    assert_eq!(snapshot.sources().len(), 5);
    assert!(
        snapshot
            .sources()
            .windows(2)
            .all(|pair| pair[0].provider <= pair[1].provider)
    );
    assert!(snapshot.sources().iter().all(|source| matches!(
        source.outcome,
        SourceOutcome::Available | SourceOutcome::Empty
    )));
}

#[test]
fn v1_controller_files_preserve_zero_and_unlimited() {
    let fixture = Fixture::new(
        900,
        "5:memory:/tenant\n4:cpu,cpuacct:/tenant\n3:pids:/tenant\n",
    );
    let memory = fixture.cgroup_root.join("memory/tenant");
    let cpu = fixture.cgroup_root.join("cpu,cpuacct/tenant");
    let pids = fixture.cgroup_root.join("pids/tenant");
    for dir in [&memory, &cpu, &pids] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(memory.join("memory.usage_in_bytes"), "0\n").unwrap();
    fs::write(memory.join("memory.limit_in_bytes"), "4096\n").unwrap();
    fs::write(cpu.join("cpu.cfs_quota_us"), "-1\n").unwrap();
    fs::write(cpu.join("cpu.cfs_period_us"), "100000\n").unwrap();
    fs::write(pids.join("pids.current"), "0\n").unwrap();
    fs::write(pids.join("pids.max"), "max\n").unwrap();

    let snapshot = collect_from_roots(&fixture.proc_dir, &fixture.cgroup_root, 11);

    assert_eq!(snapshot.current_memory_usage_bytes(), Some(0));
    assert_eq!(
        snapshot.current_memory_limit(),
        Some(LimitValue::Value(4096))
    );
    assert_eq!(
        snapshot.current_cpu_time_quota_micros(),
        Some(LimitValue::Unlimited)
    );
    assert_eq!(
        snapshot.current_process_limit(),
        Some(LimitValue::Unlimited)
    );
}

#[test]
fn absent_membership_and_missing_controller_are_distinct() {
    let absent = Fixture::new(900, "");
    let absent_snapshot = collect_from_roots(&absent.proc_dir, &absent.cgroup_root, 12);
    assert!(matches!(
        &absent_snapshot.observations().memory_usage_bytes,
        ResourceObservation::Absent { observed_at_ms: 12 }
    ));

    let unsupported = Fixture::new(900, "7:freezer:/tenant\n");
    let unsupported_snapshot =
        collect_from_roots(&unsupported.proc_dir, &unsupported.cgroup_root, 13);
    assert!(matches!(
        &unsupported_snapshot.observations().memory_usage_bytes,
        ResourceObservation::Unavailable {
            failure: FailureKind::Unsupported
        }
    ));
}

#[test]
fn one_missing_file_does_not_erase_other_current_fields() {
    let fixture = Fixture::new(900, "0::/app.scope\n");
    fixture.write_v2("/app.scope", "7\n");
    fs::remove_file(fixture.cgroup_root.join("app.scope/memory.max")).unwrap();

    let snapshot = collect_from_roots(&fixture.proc_dir, &fixture.cgroup_root, 14);

    assert_eq!(snapshot.current_memory_usage_bytes(), Some(7));
    assert_eq!(snapshot.current_memory_limit(), None);
    assert_eq!(snapshot.current_process_count(), Some(0));
    let memory_source = snapshot
        .sources()
        .iter()
        .find(|source| source.provider == MEMORY_PROVIDER)
        .unwrap();
    assert_eq!(
        memory_source.outcome,
        SourceOutcome::Partial(FailureKind::Unsupported)
    );
}

#[test]
fn permission_and_transient_io_are_not_identity_changes() {
    assert_eq!(
        field_io_failure(ErrorKind::PermissionDenied, FailureKind::IdentityChanged),
        FailureKind::PermissionDenied
    );
    assert_eq!(
        field_io_failure(ErrorKind::TimedOut, FailureKind::IdentityChanged),
        FailureKind::TemporarilyUnavailable
    );
}

#[test]
fn tracker_retains_only_same_identity_and_same_current_group() {
    let fixture = Fixture::new(900, "0::/a.scope\n");
    fixture.write_v2("/a.scope", "7\n");
    let identity = fixture.identity(900);
    let mut tracker = ProcessResourceTracker::default();
    let first = tracker.collect(&fixture.proc_dir, &fixture.cgroup_root, identity, 20);
    assert_eq!(first.current_memory_usage_bytes(), Some(7));

    fs::remove_file(fixture.cgroup_root.join("a.scope/memory.current")).unwrap();
    let stale = tracker.collect(&fixture.proc_dir, &fixture.cgroup_root, identity, 21);
    assert!(matches!(
        &stale.observations().memory_usage_bytes,
        ResourceObservation::Stale {
            last: ResourceLastObservation::Value(7),
            last_success_ms: 20,
            failure: FailureKind::Unsupported
        }
    ));

    fs::write(fixture.cgroup_root.join("a.scope/memory.current"), "9\n").unwrap();
    let recovered = tracker.collect(&fixture.proc_dir, &fixture.cgroup_root, identity, 22);
    assert_eq!(recovered.current_memory_usage_bytes(), Some(9));

    fs::remove_file(fixture.cgroup_root.join("a.scope/memory.current")).unwrap();
    fs::write(
        fixture.proc_dir.join("cgroup"),
        "0::/a.scope\nmalformed membership\n",
    )
    .unwrap();
    let partial_membership = tracker.collect(&fixture.proc_dir, &fixture.cgroup_root, identity, 23);
    assert!(matches!(
        &partial_membership.observations().resource_groups,
        ResourceObservation::Partial { .. }
    ));
    assert!(matches!(
        &partial_membership.observations().memory_usage_bytes,
        ResourceObservation::Unavailable { .. }
    ));

    fs::write(fixture.proc_dir.join("cgroup"), "0::/b.scope\n").unwrap();
    fixture.write_v2("/b.scope", "11\n");
    fs::remove_file(fixture.cgroup_root.join("b.scope/memory.current")).unwrap();
    let moved = tracker.collect(&fixture.proc_dir, &fixture.cgroup_root, identity, 24);
    assert!(matches!(
        &moved.observations().memory_usage_bytes,
        ResourceObservation::Unavailable {
            failure: FailureKind::Unsupported
        }
    ));
}

#[test]
fn pid_race_clears_cache_and_tracker_stays_bounded_to_active_target() {
    let fixture = Fixture::new(900, "0::/app.scope\n");
    fixture.write_v2("/app.scope", "1\n");
    let mut tracker = ProcessResourceTracker::default();
    let original = fixture.identity(900);
    tracker.collect(&fixture.proc_dir, &fixture.cgroup_root, original, 30);
    assert_eq!(tracker.cached_identity(), Some(original));

    fs::write(fixture.proc_dir.join("stat"), stat_text(901)).unwrap();
    let recycled = tracker.collect(&fixture.proc_dir, &fixture.cgroup_root, original, 31);
    assert!(matches!(
        &recycled.observations().memory_usage_bytes,
        ResourceObservation::Unavailable {
            failure: FailureKind::IdentityChanged
        }
    ));
    assert_eq!(tracker.cached_identity(), None);

    let replacement = fixture.identity(901);
    tracker.collect(&fixture.proc_dir, &fixture.cgroup_root, replacement, 32);
    assert_eq!(tracker.cached_identity(), Some(replacement));
}
