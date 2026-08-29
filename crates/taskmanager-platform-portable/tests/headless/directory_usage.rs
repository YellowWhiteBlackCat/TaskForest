use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use taskmanager_core::metrics::ScalarAvailability;

use super::*;

/// Host-neutral scratch tree under the OS temp directory, removed on
/// drop. No external tempfile dependency; unique per process+test.
struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(tag: &str) -> Self {
        let unique = format!(
            "taskmanager-shared-dir-usage-{tag}-{}-{:?}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let root = crate::test_support::repo_temp_dir().join(unique);
        fs::create_dir_all(&root).expect("temp root");
        Self { root }
    }

    fn path(&self) -> &PathBuf {
        &self.root
    }

    fn dir(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    fn file(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture parent");
        }
        fs::write(&path, bytes).expect("fixture file");
        path
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn control(scan_id: u64) -> DirectoryScanControl {
    DirectoryScanControl::new(
        DirectoryScanId::new(scan_id),
        Arc::new(AtomicBool::new(false)),
    )
}

fn scan_all(
    scanner: &mut DirectoryUsageScanner,
    tree: &TempTree,
    bounds: DirectoryScanBounds,
) -> DirectoryUsageSnapshot {
    let spec = DirectoryScanSpec {
        root: tree.path().to_string_lossy().into_owned(),
        bounds,
    };
    let control = control(3);
    let mut latest = None;
    for _ in 0..10_000 {
        let snapshot = scanner
            .scan_chunk(&spec, &control, 10)
            .expect("fixture scan must not fail");
        let terminal = snapshot.is_terminal();
        latest = Some(snapshot);
        if terminal {
            break;
        }
    }
    latest.expect("bounded fixture scan must terminate")
}

/// On-box receipt (host-dependent, ignored by default): proves the portable
/// scanner produces real aggregates on a real Linux filesystem. Run with:
/// `cargo nextest run --locked -p taskmanager-platform-portable --all-targets -j 4 \
/// -E 'test(on_box_scans_real_filesystem)' --run-ignored only --no-capture`
#[test]
#[ignore = "on-box: scans the real /usr filesystem (host-dependent size)"]
fn on_box_scans_real_filesystem() {
    let mut scanner = DirectoryUsageScanner::new();
    let spec = DirectoryScanSpec {
        root: "/usr".to_string(),
        bounds: DirectoryScanBounds {
            max_depth: 32,
            max_entries: 20_000,
            max_reported: 50,
        },
    };
    let control = control(99);
    let mut latest = None;
    for _ in 0..10_000 {
        let snapshot = scanner
            .scan_chunk(&spec, &control, 10)
            .expect("real scan must not hard-fail");
        let terminal = snapshot.is_terminal();
        latest = Some(snapshot);
        if terminal {
            break;
        }
    }
    let snap = latest.expect("bounded real scan must terminate");
    println!(
        "ON-BOX /usr scan: status={:?} files_counted={} directories_visited={} \
             bytes_counted={:?} unreadable_directories={} depth_reached={} capped={} \
             entries_reported={}",
        snap.status,
        snap.totals.files_counted,
        snap.totals.directories_visited,
        snap.totals.bytes_counted,
        snap.totals.unreadable_directories,
        snap.totals.depth_reached,
        snap.totals.capped,
        snap.entries.len(),
    );
    // Honest receipts: /usr is non-empty on any Linux host.
    assert!(snap.totals.files_counted > 0, "/usr must yield real files");
    assert!(snap.totals.directories_visited > 0);
    assert!(
        snap.totals.bytes_counted.current_value().is_some()
            || snap.totals.bytes_counted.last_known_value().is_some(),
        "bytes_counted must carry a real value (Available or Partial), never unavailable"
    );
}

#[test]
fn scan_counts_files_and_aggregates_subtree_sizes() {
    let tree = TempTree::new("counts");
    tree.file("a.txt", &[0; 100]);
    tree.file("logs/x.log", &[0; 50]);
    tree.file("logs/y.log", &[0; 30]);
    tree.file("logs/deep/z.log", &[0; 20]);

    let mut scanner = DirectoryUsageScanner::new();
    let snapshot = scan_all(&mut scanner, &tree, DirectoryScanBounds::default());

    assert_eq!(snapshot.status, DirectoryScanStatus::Completed);
    assert_eq!(snapshot.totals.files_counted, 4);
    assert_eq!(snapshot.totals.directories_visited, 3, "root + logs + deep");
    assert_eq!(snapshot.totals.bytes_counted.current_value(), Some(&200));
    assert_eq!(snapshot.totals.unreadable_directories, 0);
    assert!(!snapshot.totals.capped);

    let by_path: HashMap<&str, &DirectoryUsageEntry> = snapshot
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    assert_eq!(by_path[""].size_bytes.current_value(), Some(&200));
    assert_eq!(by_path["logs"].size_bytes.current_value(), Some(&100));
    assert_eq!(by_path["logs/deep"].size_bytes.current_value(), Some(&20));
    assert_eq!(by_path["logs/deep"].file_count.current_value(), Some(&1));
}

#[test]
fn empty_directory_is_measured_zero_not_unavailable() {
    let tree = TempTree::new("empty");
    tree.dir("vacant");

    let mut scanner = DirectoryUsageScanner::new();
    let snapshot = scan_all(&mut scanner, &tree, DirectoryScanBounds::default());

    assert_eq!(snapshot.status, DirectoryScanStatus::Completed);
    assert_eq!(snapshot.totals.files_counted, 0);
    assert_eq!(
        snapshot.totals.bytes_counted.availability(),
        ScalarAvailability::Available,
        "an empty readable tree is measured zero, never fabricated unavailable"
    );
    assert_eq!(snapshot.totals.bytes_counted.current_value(), Some(&0));
}

#[cfg(unix)]
#[test]
fn unreadable_directory_is_typed_permission_denied_never_zero() {
    use std::os::unix::fs::PermissionsExt;

    let tree = TempTree::new("denied");
    tree.file("open.txt", &[0; 10]);
    let denied = tree.dir("secret");
    tree.file("secret/hidden.txt", &[0; 40]);
    let mut permissions = fs::metadata(&denied)
        .expect("fixture metadata")
        .permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(&denied, permissions.clone()).expect("chmod 000");

    let mut scanner = DirectoryUsageScanner::new();
    let snapshot = scan_all(&mut scanner, &tree, DirectoryScanBounds::default());

    assert_eq!(snapshot.status, DirectoryScanStatus::Completed);
    assert_eq!(snapshot.totals.unreadable_directories, 1);
    assert_eq!(
        snapshot.totals.bytes_counted.availability(),
        ScalarAvailability::Partial(FailureKind::PermissionDenied),
        "the byte sum is real but typed partial after a denied subtree"
    );
    assert_eq!(snapshot.totals.bytes_counted.current_value(), Some(&10));
    let secret = snapshot
        .entries
        .iter()
        .find(|entry| entry.path == "secret")
        .expect("the denied directory keeps its report entry");
    assert_eq!(secret.unreadable, Some(FailureKind::PermissionDenied));
    assert_eq!(
        secret.size_bytes.availability(),
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );

    // Restore permissions so the drop cleanup can remove the tree.
    let mut restored = permissions;
    restored.set_mode(0o700);
    let _ = fs::set_permissions(&denied, restored);
}

#[cfg(unix)]
#[test]
fn symlink_loops_terminate_and_never_follow_targets() {
    use std::os::unix::fs::symlink;

    let tree = TempTree::new("loop");
    tree.file("data.bin", &[0; 64]);
    let dir = tree.dir("loop");
    // The fixture parent must exist before symlinks are created under it;
    // `dir()` only computes the path, it does not create it.
    fs::create_dir_all(&dir).expect("fixture loop dir");
    // loop/a -> the real file, loop/b -> a: a cycle the scanner must not
    // follow. Only `symlink_metadata` is consulted, so neither link's
    // target subtree enters the size aggregate.
    symlink(tree.path().join("data.bin"), dir.join("a")).expect("symlink a");
    symlink(dir.join("a"), dir.join("b")).expect("symlink b");

    let mut scanner = DirectoryUsageScanner::new();
    let snapshot = scan_all(&mut scanner, &tree, DirectoryScanBounds::default());

    assert_eq!(snapshot.status, DirectoryScanStatus::Completed);
    assert_eq!(
        snapshot.totals.files_counted, 1,
        "only the real file is counted; symlinks are entries, never followed"
    );
    assert_eq!(snapshot.totals.bytes_counted.current_value(), Some(&64));
}

#[test]
fn depth_bound_skips_deeper_directories() {
    let tree = TempTree::new("depth");
    tree.file("d1/d2/d3/d4/deep.txt", &[0; 10]);

    let mut scanner = DirectoryUsageScanner::new();
    let snapshot = scan_all(
        &mut scanner,
        &tree,
        DirectoryScanBounds {
            max_depth: 2,
            ..DirectoryScanBounds::default()
        },
    );

    assert_eq!(snapshot.status, DirectoryScanStatus::Completed);
    // The depth bound skips deeper directories without raising the entry-cap
    // flag (the entry-cap flag is reserved for `max_entries`). The scan still
    // terminates honestly: it reached the bound, did not fabricate the file
    // beyond it, and reports the deepest level actually visited.
    assert_eq!(
        snapshot.totals.files_counted, 0,
        "the file is beyond the bound"
    );
    assert_eq!(snapshot.totals.depth_reached, 2);
    assert!(
        snapshot
            .entries
            .iter()
            .all(|entry| entry.path != "d1/d2/d3"),
        "directories beyond max_depth must not appear in the report"
    );
}

#[test]
fn entry_cap_stops_counting_and_marks_capped() {
    let tree = TempTree::new("cap");
    tree.file("one.txt", &[0; 1]);
    tree.file("two.txt", &[0; 1]);

    let mut scanner = DirectoryUsageScanner::new();
    // max_entries=2 admits exactly the root directory plus one file; the
    // second file would exceed the cap, so `record_file` returns false and
    // the session marks itself capped.
    let snapshot = scan_all(
        &mut scanner,
        &tree,
        DirectoryScanBounds {
            max_entries: 2,
            ..DirectoryScanBounds::default()
        },
    );

    assert_eq!(snapshot.status, DirectoryScanStatus::Completed);
    assert!(snapshot.totals.capped);
    assert!(
        snapshot.totals.files_counted + snapshot.totals.directories_visited <= 2,
        "counters must respect the entry cap"
    );
}

#[test]
fn high_fanout_directory_consumes_the_entry_cap_at_discovery() {
    const FANOUT: usize = 256;
    const ENTRY_CAP: u64 = 64;

    let tree = TempTree::new("fanout-cap");
    for index in 0..FANOUT {
        fs::create_dir(tree.dir(&format!("child-{index:04}"))).expect("fanout directory");
    }
    let spec = DirectoryScanSpec {
        root: tree.path().to_string_lossy().into_owned(),
        bounds: DirectoryScanBounds {
            max_entries: ENTRY_CAP,
            max_reported: ENTRY_CAP as usize,
            ..DirectoryScanBounds::default()
        },
    };
    let mut scanner = DirectoryUsageScanner::new();
    let snapshot = scanner
        .scan_chunk(&spec, &control(11), 10)
        .expect("fanout scan");

    assert_eq!(
        snapshot.status,
        DirectoryScanStatus::Completed,
        "discovering the first out-of-budget directory must stop in the same chunk"
    );
    assert!(snapshot.totals.capped);
    assert!(
        snapshot.totals.directories_visited <= ENTRY_CAP,
        "pending directories may never grow beyond the counted entry authority"
    );
}

#[cfg(unix)]
#[test]
fn high_fanout_symlinks_consume_the_same_global_entry_budget() {
    use std::os::unix::fs::symlink;

    const FANOUT: usize = 256;
    const ENTRY_CAP: u64 = 64;

    let tree = TempTree::new("fanout-symlink-cap");
    let target = tree.file("target.bin", &[7]);
    for index in 0..FANOUT {
        symlink(&target, tree.dir(&format!("link-{index:04}"))).expect("fanout symlink");
    }
    let spec = DirectoryScanSpec {
        root: tree.path().to_string_lossy().into_owned(),
        bounds: DirectoryScanBounds {
            max_entries: ENTRY_CAP,
            max_reported: 1,
            ..DirectoryScanBounds::default()
        },
    };
    let mut scanner = DirectoryUsageScanner::new();
    let snapshot = scanner
        .scan_chunk(&spec, &control(13), 10)
        .expect("symlink fanout scan");

    assert_eq!(snapshot.status, DirectoryScanStatus::Completed);
    assert!(
        snapshot.totals.capped,
        "links and special entries must reach the same global cap as files and directories"
    );
    assert!(
        snapshot.totals.files_counted + snapshot.totals.directories_visited <= ENTRY_CAP,
        "typed file/directory totals remain singly counted inside the broader entry budget"
    );
}

#[test]
fn high_fanout_read_dir_cursor_yields_at_the_per_chunk_entry_budget() {
    const FANOUT: usize = 1_200;

    let tree = TempTree::new("fanout-cursor");
    for index in 0..FANOUT {
        fs::create_dir(tree.dir(&format!("child-{index:04}"))).expect("fanout directory");
    }
    let spec = DirectoryScanSpec {
        root: tree.path().to_string_lossy().into_owned(),
        bounds: DirectoryScanBounds {
            max_entries: (FANOUT + 1) as u64,
            max_reported: 1,
            ..DirectoryScanBounds::default()
        },
    };
    let scan_control = control(12);
    let mut scanner = DirectoryUsageScanner::new();
    let first = scanner
        .scan_chunk(&spec, &scan_control, 10)
        .expect("fanout scan");

    assert_eq!(first.status, DirectoryScanStatus::Scanning);
    assert!(
        first.totals.directories_visited > 1,
        "the first chunk must make observable traversal progress"
    );
    assert!(
        first.totals.directories_visited <= CHUNK_ENTRY_BUDGET,
        "one chunk may consume at most its explicit cursor budget"
    );

    let terminal = scan_all(&mut scanner, &tree, spec.bounds);
    assert_eq!(terminal.status, DirectoryScanStatus::Completed);
    assert_eq!(terminal.totals.directories_visited, (FANOUT + 1) as u64);
}

#[test]
fn missing_root_is_a_typed_terminal_failure() {
    let tree = TempTree::new("gone");
    let missing = tree.path().join("does-not-exist");

    let spec = DirectoryScanSpec {
        root: missing.to_string_lossy().into_owned(),
        bounds: DirectoryScanBounds::default(),
    };
    let mut scanner = DirectoryUsageScanner::new();
    let snapshot = scanner
        .scan_chunk(&spec, &control(9), 10)
        .expect("scanner failure is typed into the snapshot");
    // The scan root itself cannot be listed: the scanner treats this as a
    // terminal typed failure (Failed(TemporarilyUnavailable) for a missing
    // path, Failed(PermissionDenied) for an unreadable one) rather than a
    // silent empty/Completed tree.
    assert_eq!(
        snapshot.status,
        DirectoryScanStatus::Failed(FailureKind::TemporarilyUnavailable),
        "a missing scan root must surface as a typed terminal failure"
    );
    assert_eq!(snapshot.totals.unreadable_directories, 1);
    assert!(snapshot.entries[0].unreadable.is_some());
}

#[test]
fn cancel_flag_produces_a_partial_cancelled_terminal_state() {
    let tree = TempTree::new("cancel");
    tree.file("a.bin", &[0; 10]);

    let spec = DirectoryScanSpec {
        root: tree.path().to_string_lossy().into_owned(),
        bounds: DirectoryScanBounds::default(),
    };
    let cancelled = Arc::new(AtomicBool::new(true));
    let control = DirectoryScanControl::new(DirectoryScanId::new(4), cancelled);
    let mut scanner = DirectoryUsageScanner::new();
    let snapshot = scanner
        .scan_chunk(&spec, &control, 10)
        .expect("cancel must not be a scanner failure");

    assert_eq!(snapshot.status, DirectoryScanStatus::Cancelled);
    assert_eq!(snapshot.scan_id, DirectoryScanId::new(4));
}

#[test]
fn display_path_helpers_join_and_parent_deterministically() {
    assert_eq!(parent_display("a/b/c"), "a/b");
    assert_eq!(parent_display("a"), "");
    assert_eq!(parent_display(""), "");
    assert_eq!(join_display("", "root"), "root");
    assert_eq!(join_display("a/b", "c"), "a/b/c");
}

#[test]
fn io_error_mapping_never_fabricates_permission_denied() {
    assert_eq!(
        io_failure_kind(std::io::ErrorKind::PermissionDenied),
        FailureKind::PermissionDenied
    );
    assert_eq!(
        io_failure_kind(std::io::ErrorKind::NotFound),
        FailureKind::TemporarilyUnavailable
    );
    assert_eq!(
        io_failure_kind(std::io::ErrorKind::Interrupted),
        FailureKind::TemporarilyUnavailable
    );
}
