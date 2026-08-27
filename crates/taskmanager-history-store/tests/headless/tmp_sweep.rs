//! Behavior tests for stale-temporary hygiene on the flush/scan path: an
//! abandoned `.tmp<pid>-<seq>` sibling is swept only when its writer is
//! provably dead AND the file is provably old, and accumulated debris no
//! longer stalls persistence against the directory-entry bound.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use taskmanager_core::{HistoricalSample, HistoryMetric, HistoryRecordSink, HistorySeriesKey};
use taskmanager_history_store::{
    FlushReport, MAX_DIRECTORY_ENTRIES_PER_SCAN, PersistentHistoryStore, RetentionPolicy,
    STALE_TEMPORARY_AGE_MS,
};

/// Pids at or beyond 4e9 exceed every supported pid space, so they are
/// provably dead; every other pid — including this test process — is alive.
fn only_impossible_pids_are_gone(pid: u32) -> bool {
    pid >= 4_000_000_000
}
const IMPOSSIBLE_PID_IS_GONE: fn(u32) -> bool = only_impossible_pids_are_gone;

const DEAD_PID: u32 = 4_000_000_000;

fn fixture_root(tag: &str) -> PathBuf {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-history-store-sweep-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn cleanup(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
}

fn cpu_key() -> HistorySeriesKey {
    HistorySeriesKey::system(HistoryMetric::CpuUsagePct)
}

fn record_one_sample(store: &PersistentHistoryStore) {
    HistoryRecordSink::record_sample(
        store,
        cpu_key(),
        HistoricalSample {
            revision: 1,
            completed_at_ms: 1_000,
            measured_at_ms: Some(1_000),
            value: Some(12.0),
        },
    );
}

/// Plant one abandoned temporary. `Some(age)` backdates the mtime (planted
/// files default to "just now", which must survive every sweep).
fn plant_temporary(root: &Path, file_name: &str, age: Option<Duration>) {
    let path = root.join(file_name);
    std::fs::write(&path, b"abandoned").expect("plant temporary");
    if let Some(age) = age {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open planted temporary for backdating")
            .set_modified(SystemTime::now() - age)
            .expect("backdate planted temporary");
    }
}

/// One hour past the stale threshold, so age verdicts never sit on the
/// boundary.
fn stale_age() -> Duration {
    Duration::from_millis(STALE_TEMPORARY_AGE_MS) + Duration::from_secs(3_600)
}

fn planted(root: &Path, file_name: &str) -> bool {
    root.join(file_name).exists()
}

#[test]
fn flush_sweeps_a_temporary_only_when_its_writer_is_dead_and_it_is_old() {
    let root = fixture_root("criteria");
    let store = PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(u64::MAX, u64::MAX),
        IMPOSSIBLE_PID_IS_GONE,
    )
    .expect("open store");
    record_one_sample(&store);

    let old = Some(stale_age());
    let dead_old_series = format!("cpu.jsonl.tmp{DEAD_PID}-7");
    let dead_old_boot = format!("boot-evidence.json.tmp{DEAD_PID}-1");
    let dead_fresh = format!("mem.jsonl.tmp{DEAD_PID}-2");
    let live_old = format!("disk.jsonl.tmp{}-3", std::process::id());
    plant_temporary(&root, &dead_old_series, old);
    plant_temporary(&root, &dead_old_boot, old);
    plant_temporary(&root, &dead_fresh, None);
    plant_temporary(&root, &live_old, old);
    std::fs::write(root.join("external-debris.junk"), b"debris").expect("plant foreign debris");

    let report = store.flush(1_000).expect("flush");
    assert_eq!(
        report,
        FlushReport {
            appended_series: 1,
            appended_samples: 1,
            ttl_trimmed_files: 0,
            quota_trimmed_files: 0,
            stale_temporaries_swept: 2,
            temporary_sweep_failures: 0,
        }
    );

    assert!(
        !planted(&root, &dead_old_series),
        "dead writer + old mtime is swept"
    );
    assert!(
        !planted(&root, &dead_old_boot),
        "boot-evidence temporaries share the same sweep"
    );
    assert!(
        planted(&root, &dead_fresh),
        "a fresh temporary may still be mid-write"
    );
    assert!(
        planted(&root, &live_old),
        "a live writer's temporary is never touched"
    );
    assert!(
        planted(&root, "external-debris.junk"),
        "foreign debris is not ours to delete"
    );
    assert!(
        planted(&root, "history.lock"),
        "the single-writer claim survives the sweep"
    );
    assert!(
        planted(&root, &format!("{}.jsonl", cpu_key().file_stem())),
        "series data survives the sweep"
    );
    drop(store);
    cleanup(&root);
}

#[test]
fn accumulated_stale_temporaries_no_longer_stall_persistence() {
    let root = fixture_root("debris-recovery");
    std::fs::create_dir_all(&root).expect("create root");
    let planted_count = MAX_DIRECTORY_ENTRIES_PER_SCAN + 1;
    for index in 0..planted_count {
        plant_temporary(
            &root,
            &format!("flood-{index}.jsonl.tmp{DEAD_PID}-{index}"),
            Some(stale_age()),
        );
    }
    let store = PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(u64::MAX, u64::MAX),
        IMPOSSIBLE_PID_IS_GONE,
    )
    .expect("open store under debris");
    record_one_sample(&store);

    let first = store
        .flush(1_000)
        .expect("debris no longer fails the flush");
    // One bounded pass examines at most the scan bound of entries; the lock
    // and the freshly appended series file may sit inside that window.
    assert!(
        first.stale_temporaries_swept >= MAX_DIRECTORY_ENTRIES_PER_SCAN - 2,
        "the pass must use (nearly) its whole entry budget, swept: {}",
        first.stale_temporaries_swept
    );
    assert!(first.stale_temporaries_swept <= MAX_DIRECTORY_ENTRIES_PER_SCAN);
    assert_eq!(first.temporary_sweep_failures, 0);
    assert_eq!(first.appended_samples, 1, "persistence itself kept working");

    let second = store.flush(2_000).expect("second flush");
    assert_eq!(
        first.stale_temporaries_swept + second.stale_temporaries_swept,
        planted_count,
        "consecutive flushes finish the recovery"
    );
    let remaining = std::fs::read_dir(&root)
        .expect("list root")
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with("flood-"))
        })
        .count();
    assert_eq!(remaining, 0, "all planted debris is eventually swept");
    drop(store);
    cleanup(&root);
}
