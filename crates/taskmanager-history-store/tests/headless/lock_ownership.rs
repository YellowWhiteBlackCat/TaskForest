//! Behavioral contracts for fail-closed single-writer ownership.

use taskmanager_history_store::{
    HistoryStoreErrorKind, HistoryWriterClaimStatus, PersistentHistoryStore, RetentionPolicy,
    TRIM_INTERVAL_MS, probe_root_lock,
};

fn alive_probe(_pid: u32) -> bool {
    false
}

#[test]
fn read_only_probe_distinguishes_absent_live_stale_and_ambiguous_claims() {
    let root = fixture_root("claim-probe");
    assert_eq!(
        probe_root_lock(&root, ALIVE),
        HistoryWriterClaimStatus::Absent
    );

    let store = PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
        ALIVE,
    )
    .expect("open live owner");
    assert!(matches!(
        probe_root_lock(&root, ALIVE),
        HistoryWriterClaimStatus::Live { pid } if pid == std::process::id()
    ));
    assert!(matches!(
        probe_root_lock(&root, |_| true),
        HistoryWriterClaimStatus::Stale { pid } if pid == std::process::id()
    ));
    drop(store);

    std::fs::create_dir_all(&root).expect("restore history root");
    std::fs::write(root.join("history.lock"), "not-a-claim").expect("write malformed claim");
    assert_eq!(
        probe_root_lock(&root, ALIVE),
        HistoryWriterClaimStatus::Ambiguous
    );
    cleanup(&root);
}
const ALIVE: fn(u32) -> bool = alive_probe;

fn fixture_root(tag: &str) -> std::path::PathBuf {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-history-store-lock-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn cleanup(root: &std::path::Path) {
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_second_live_instance_is_refused_until_the_first_releases() {
    let root = fixture_root("live-holder");
    let policy = RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX);
    let first = PersistentHistoryStore::open(&root, policy, ALIVE).expect("first owner");
    match PersistentHistoryStore::open(&root, policy, |_pid| true) {
        Err(error) => assert_eq!(error.kind(), HistoryStoreErrorKind::Locked),
        Ok(second) => {
            drop(second);
            panic!("the held OS lock must override a false stale-PID verdict");
        }
    }

    drop(first);
    let reopened = PersistentHistoryStore::open(&root, policy, ALIVE)
        .expect("the exact owner drop releases its claim");
    drop(reopened);
    cleanup(&root);
}

#[test]
fn a_parseable_stale_pid_is_recovered_and_replaced_by_a_live_claim() {
    for (tag, stale_claim) in [
        ("legacy-stale-holder", "4000000000"),
        ("token-stale-holder", "4000000000:17"),
    ] {
        let root = fixture_root(tag);
        std::fs::create_dir_all(&root).expect("create history root");
        let lock = root.join("history.lock");
        std::fs::write(&lock, stale_claim).expect("write stale claim");
        let owner = PersistentHistoryStore::open(
            &root,
            RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
            |_pid| true,
        )
        .expect("provably dead PID is recoverable");
        assert_ne!(
            std::fs::read_to_string(&lock).expect("read replacement claim"),
            stale_claim
        );
        match PersistentHistoryStore::open(
            &root,
            RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
            ALIVE,
        ) {
            Err(error) => assert_eq!(error.kind(), HistoryStoreErrorKind::Locked),
            Ok(second) => {
                drop(second);
                panic!("the recovered live claim must remain exclusive");
            }
        }
        drop(owner);
        cleanup(&root);
    }
}

#[test]
fn a_stranded_claim_from_this_process_is_retaken_by_a_winning_try_lock() {
    let root = fixture_root("same-process-stranded-claim");
    std::fs::create_dir_all(&root).expect("create history root");
    let lock = root.join("history.lock");
    let stranded = format!("{}:1", std::process::id());
    std::fs::write(&lock, &stranded).expect("write stranded same-process claim");

    // ALIVE insists every holder lives, so the claim text alone can never
    // authorize this takeover; a winning try_lock is the only proof that
    // the previous generation released and merely stranded its claim.
    let owner = PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
        ALIVE,
    )
    .expect("a claim stranded by this process is retaken through the file lock");
    assert_ne!(
        std::fs::read_to_string(&lock).expect("read replacement claim"),
        stranded
    );
    match PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
        ALIVE,
    ) {
        Err(error) => assert_eq!(error.kind(), HistoryStoreErrorKind::Locked),
        Ok(second) => {
            drop(second);
            panic!("the retaken live claim must remain exclusive");
        }
    }
    drop(owner);
    cleanup(&root);
}

#[test]
fn a_foreign_live_claim_with_a_free_file_lock_still_fails_closed() {
    let root = fixture_root("foreign-live-claim");
    std::fs::create_dir_all(&root).expect("create history root");
    let lock = root.join("history.lock");
    let foreign = "4000000000:2";
    std::fs::write(&lock, foreign).expect("write foreign live claim");

    match PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
        |pid| pid != 4000000000,
    ) {
        Err(error) => assert_eq!(error.kind(), HistoryStoreErrorKind::Locked),
        Ok(store) => {
            drop(store);
            panic!("a foreign claim is not proof that its holder died");
        }
    }
    assert_eq!(
        std::fs::read_to_string(&lock).expect("foreign claim remains"),
        foreign
    );
    cleanup(&root);
}

#[test]
fn torn_or_unparseable_claims_fail_closed_without_being_rewritten() {
    for (tag, content) in [
        ("empty", ""),
        ("malformed-pid", "not-a-pid"),
        ("malformed-token", "123:not-a-sequence"),
    ] {
        let root = fixture_root(tag);
        std::fs::create_dir_all(&root).expect("create history root");
        let lock = root.join("history.lock");
        std::fs::write(&lock, content).expect("write ambiguous claim");
        match PersistentHistoryStore::open(
            &root,
            RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
            |_pid| true,
        ) {
            Err(error) => assert_eq!(error.kind(), HistoryStoreErrorKind::Locked),
            Ok(store) => {
                drop(store);
                panic!("ambiguous ownership is not proof that the holder died");
            }
        }
        assert_eq!(
            std::fs::read_to_string(&lock).expect("ambiguous claim remains"),
            content
        );
        cleanup(&root);
    }

    let root = fixture_root("oversized-claim");
    std::fs::create_dir_all(&root).expect("create history root");
    let lock = root.join("history.lock");
    std::fs::write(&lock, "9".repeat(129)).expect("write oversized claim");
    match PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
        |_pid| true,
    ) {
        Err(error) => assert_eq!(error.kind(), HistoryStoreErrorKind::Locked),
        Ok(store) => {
            drop(store);
            panic!("an oversized claim is ambiguous and must remain held");
        }
    }
    assert_eq!(std::fs::metadata(&lock).expect("claim remains").len(), 129);
    cleanup(&root);
}

#[test]
fn old_owner_drop_does_not_remove_a_replacement_claim() {
    let root = fixture_root("replacement-owner");
    let store = PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
        ALIVE,
    )
    .expect("open owner");
    let lock = root.join("history.lock");
    let replacement = "424242:replacement-owner";
    std::fs::write(&lock, replacement).expect("replace owner claim externally");

    drop(store);
    assert_eq!(
        std::fs::read_to_string(&lock).expect("replacement claim survives old drop"),
        replacement
    );
    cleanup(&root);
}
