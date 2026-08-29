use std::sync::{Arc, Barrier, Mutex};

use super::SequenceCommitter;
use taskmanager_platform_contract::EventSequence;

#[test]
fn sequence_authority_remains_locked_until_the_queue_commit_finishes() {
    let committer = Arc::new(SequenceCommitter::default());
    let entered_commit = Arc::new(Barrier::new(2));
    let release_commit = Arc::new(Barrier::new(2));
    let observed = Arc::new(Mutex::new(Vec::new()));

    let worker_committer = committer.clone();
    let worker_entered = entered_commit.clone();
    let worker_release = release_commit.clone();
    let worker_observed = observed.clone();
    let first = std::thread::spawn(move || {
        worker_committer
            .commit(|sequence| {
                worker_entered.wait();
                worker_release.wait();
                worker_observed
                    .lock()
                    .expect("observed sequence lock")
                    .push(sequence);
            })
            .expect("first sequence commit");
    });

    entered_commit.wait();
    assert!(
        committer.sequence.try_lock().is_err(),
        "sequence allocation must remain serialized while its queue commit is paused"
    );
    release_commit.wait();
    first.join().expect("first publisher");

    committer
        .commit(|sequence| {
            observed
                .lock()
                .expect("observed sequence lock")
                .push(sequence);
        })
        .expect("second sequence commit");
    assert_eq!(
        *observed.lock().expect("observed sequence lock"),
        [EventSequence::new(1), EventSequence::new(2)]
    );
}
