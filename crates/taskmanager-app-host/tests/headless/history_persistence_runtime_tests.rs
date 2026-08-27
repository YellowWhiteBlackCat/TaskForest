use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use taskmanager_application::{HistoryMetric, HistorySeriesKey};
use taskmanager_core::{HistoricalSample, HistoryRecordSink};
use taskmanager_history_store::HistoryQuery;

use super::{
    BackendRecordOutcome, HistoryPersistenceBackend, HistoryPersistenceCoordinator,
    HistoryPersistenceShutdownOutcome, HistoryPersistenceStartErrorKind,
    HistoryPersistenceWorkerState, RecordCommand, flush_backend, record_backend,
    saturating_increment,
};

struct RecordingBackend {
    record_tx: std::sync::mpsc::Sender<(HistorySeriesKey, HistoricalSample)>,
}

impl HistoryPersistenceBackend for RecordingBackend {
    fn record(&mut self, command: RecordCommand) -> BackendRecordOutcome {
        let _ = self.record_tx.send((command.key, command.sample));
        BackendRecordOutcome::Accepted
    }

    fn flush(&mut self, _now_ms: u64) -> Result<(), taskmanager_history_store::HistoryStoreError> {
        Ok(())
    }
}

#[test]
fn admitted_records_cross_only_the_bounded_typed_port() {
    let (record_tx, record_rx) = std::sync::mpsc::channel();
    let coordinator = HistoryPersistenceCoordinator::start_with_backend(
        Box::new(RecordingBackend { record_tx }),
        Duration::from_secs(60),
    )
    .expect("start fixture worker");
    let sink = coordinator.record_sink();
    let key = HistorySeriesKey::system(HistoryMetric::CpuUsagePct);
    let sample = HistoricalSample {
        revision: 1,
        completed_at_ms: 20,
        measured_at_ms: Some(20),
        value: Some(42.0),
    };
    HistoryRecordSink::record_sample(sink.as_ref(), key.clone(), sample);
    assert_eq!(
        record_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker receives admitted record"),
        (key, sample)
    );
}

struct BlockingFlushBackend {
    entered: Arc<Barrier>,
    release: Arc<Barrier>,
}

impl HistoryPersistenceBackend for BlockingFlushBackend {
    fn record(&mut self, _command: RecordCommand) -> BackendRecordOutcome {
        BackendRecordOutcome::Accepted
    }

    fn flush(&mut self, _now_ms: u64) -> Result<(), taskmanager_history_store::HistoryStoreError> {
        self.entered.wait();
        self.release.wait();
        Ok(())
    }
}

#[test]
fn blocked_shutdown_flush_detaches_at_the_bounded_owner_seam() {
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let coordinator = HistoryPersistenceCoordinator::start_with_backend(
        Box::new(BlockingFlushBackend {
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        }),
        Duration::from_secs(60),
    )
    .expect("start fixture worker");
    let monitor = coordinator.monitor();
    let start = Instant::now();
    let drop_thread = std::thread::spawn(move || drop(coordinator));
    entered.wait();
    drop_thread.join().expect("bounded coordinator drop");
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(
        monitor.status().worker,
        HistoryPersistenceWorkerState::Detached
    );
    release.wait();
}

struct FlushSignalingBackend {
    flush_tx: std::sync::mpsc::Sender<()>,
}

impl HistoryPersistenceBackend for FlushSignalingBackend {
    fn record(&mut self, _command: RecordCommand) -> BackendRecordOutcome {
        BackendRecordOutcome::Accepted
    }

    fn flush(&mut self, _now_ms: u64) -> Result<(), taskmanager_history_store::HistoryStoreError> {
        let _ = self.flush_tx.send(());
        Ok(())
    }
}

#[test]
fn continuous_record_admission_cannot_starve_the_flush_cadence() {
    let (flush_tx, flush_rx) = std::sync::mpsc::channel();
    let coordinator = HistoryPersistenceCoordinator::start_with_backend(
        Box::new(FlushSignalingBackend { flush_tx }),
        Duration::from_millis(5),
    )
    .expect("start fixture worker");
    let sink = coordinator.record_sink();
    let producer = std::thread::spawn(move || {
        let key = HistorySeriesKey::system(HistoryMetric::CpuUsagePct);
        let until = Instant::now() + Duration::from_millis(100);
        let mut revision = 1u64;
        while Instant::now() < until {
            sink.record_sample(
                key.clone(),
                HistoricalSample {
                    revision,
                    completed_at_ms: revision,
                    measured_at_ms: Some(revision),
                    value: Some(1.0),
                },
            );
            revision = revision.saturating_add(1);
        }
    });
    flush_rx
        .recv_timeout(Duration::from_millis(80))
        .expect("flush remains schedulable under a hot record lane");
    producer.join().expect("record producer");
}

#[test]
fn blocked_store_open_has_a_bounded_typed_startup_fallback() {
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let opener = move || {
        let _ = release_rx.recv();
        let (flush_tx, _flush_rx) = std::sync::mpsc::channel();
        HistoryPersistenceCoordinator::start_with_backend(
            Box::new(FlushSignalingBackend { flush_tx }),
            Duration::from_secs(60),
        )
    };
    let started = Instant::now();
    let error = HistoryPersistenceCoordinator::start_with_opener(opener, Duration::from_millis(20))
        .expect_err("blocked open must time out");
    assert_eq!(
        error.kind(),
        HistoryPersistenceStartErrorKind::BootstrapTimedOut
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    release_tx.send(()).expect("release detached bootstrap");
}

struct FailingHealthBackend;

impl HistoryPersistenceBackend for FailingHealthBackend {
    fn record(&mut self, _command: RecordCommand) -> BackendRecordOutcome {
        BackendRecordOutcome::Rejected(Arc::from("fixture record limit"))
    }

    fn flush(&mut self, _now_ms: u64) -> Result<(), taskmanager_history_store::HistoryStoreError> {
        Err(taskmanager_history_store::HistoryStoreError::new(
            taskmanager_history_store::HistoryStoreErrorKind::Write,
            "界".repeat(600),
        ))
    }
}

#[test]
fn record_rejection_and_flush_failure_remain_typed_in_runtime_health() {
    let coordinator = HistoryPersistenceCoordinator::start_with_backend(
        Box::new(FailingHealthBackend),
        Duration::from_millis(5),
    )
    .expect("start fixture worker");
    coordinator.record_sink().record_sample(
        HistorySeriesKey::system(HistoryMetric::CpuUsagePct),
        HistoricalSample {
            revision: 1,
            completed_at_ms: 1,
            measured_at_ms: Some(1),
            value: Some(1.0),
        },
    );
    let monitor = coordinator.monitor();
    let deadline = Instant::now() + Duration::from_secs(1);
    let status = loop {
        let status = monitor.status();
        if status.store_rejections == 1 && status.flush_failures > 0 {
            break status;
        }
        assert!(Instant::now() < deadline, "health publication timed out");
        std::thread::yield_now();
    };
    let record_failure = status.record_failure.expect("record failure partition");
    assert_eq!(
        record_failure.operation,
        super::HistoryPersistenceOperation::Record
    );
    let failure = status.flush_failure.expect("flush failure partition");
    assert_eq!(failure.operation, super::HistoryPersistenceOperation::Flush);
    assert_eq!(failure.kind, super::HistoryPersistenceFailureKind::Write);
    assert_eq!(failure.detail.chars().count(), 512);
}

#[test]
fn ingress_drop_counter_saturates_instead_of_wrapping() {
    let counter = std::sync::atomic::AtomicU64::new(u64::MAX);
    saturating_increment(&counter);
    assert_eq!(counter.load(std::sync::atomic::Ordering::Acquire), u64::MAX);
}

struct RecoveryBackend {
    record_outcomes: std::collections::VecDeque<BackendRecordOutcome>,
    flush_outcomes:
        std::collections::VecDeque<Result<(), taskmanager_history_store::HistoryStoreError>>,
}

impl HistoryPersistenceBackend for RecoveryBackend {
    fn record(&mut self, _command: RecordCommand) -> BackendRecordOutcome {
        self.record_outcomes
            .pop_front()
            .unwrap_or(BackendRecordOutcome::Accepted)
    }

    fn flush(&mut self, _now_ms: u64) -> Result<(), taskmanager_history_store::HistoryStoreError> {
        self.flush_outcomes.pop_front().unwrap_or(Ok(()))
    }
}

fn record_command(revision: u64) -> RecordCommand {
    RecordCommand {
        key: HistorySeriesKey::system(HistoryMetric::CpuUsagePct),
        sample: HistoricalSample {
            revision,
            completed_at_ms: revision,
            measured_at_ms: Some(revision),
            value: Some(1.0),
        },
    }
}

#[test]
fn record_and_flush_failures_recover_as_independent_partitions() {
    let mut backend = RecoveryBackend {
        record_outcomes: std::collections::VecDeque::from([
            BackendRecordOutcome::Rejected(Arc::from("record failure")),
            BackendRecordOutcome::Accepted,
        ]),
        flush_outcomes: std::collections::VecDeque::from([
            Err(taskmanager_history_store::HistoryStoreError::new(
                taskmanager_history_store::HistoryStoreErrorKind::Write,
                "flush failure",
            )),
            Ok(()),
        ]),
    };
    let health = std::sync::Mutex::new(super::HistoryPersistenceHealth::default());

    record_backend(&mut backend, record_command(1), &health);
    flush_backend(&mut backend, &health);
    {
        let health = health.lock().expect("health");
        assert!(health.record_failure.is_some());
        assert!(health.flush_failure.is_some());
    }

    flush_backend(&mut backend, &health);
    {
        let health = health.lock().expect("health");
        assert!(health.record_failure.is_some());
        assert!(health.flush_failure.is_none());
    }
    record_backend(&mut backend, record_command(2), &health);
    let health = health.lock().expect("health");
    assert!(health.record_failure.is_none());
    assert!(health.flush_failure.is_none());
}

#[test]
fn replay_during_writer_flush_never_poisons_the_later_complete_read() {
    static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let fixture_id = NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp/app-host-history-writer")
        .join(format!("{}-{fixture_id}", std::process::id()));
    let coordinator = HistoryPersistenceCoordinator::start_path_with_interval(
        &root,
        |_| false,
        Duration::from_millis(1),
    )
    .expect("start real writer");
    let sink = coordinator.record_sink();
    let monitor = coordinator.monitor();
    let key = HistorySeriesKey::system(HistoryMetric::CpuUsagePct);
    let base_ms = super::unix_now_ms();
    for revision in 1..=500u64 {
        sink.record_sample(
            key.clone(),
            HistoricalSample {
                revision,
                completed_at_ms: base_ms.saturating_add(revision),
                measured_at_ms: Some(base_ms.saturating_add(revision)),
                value: Some(1.0),
            },
        );
        if revision.is_multiple_of(17) {
            let _ = HistoryQuery::new(&root).series(
                &key,
                taskmanager_application::HistoryWindow::OneHour,
                base_ms.saturating_add(1_000),
            );
        }
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while monitor.status().flushes == 0 {
        assert!(Instant::now() < deadline, "real writer never flushed");
        std::thread::yield_now();
    }
    drop(sink);
    drop(coordinator);

    let final_read = HistoryQuery::new(&root)
        .series(
            &key,
            taskmanager_application::HistoryWindow::OneHour,
            base_ms.saturating_add(1_000),
        )
        .expect("final replay read")
        .expect("persisted series");
    assert_eq!(final_read.corrupt_lines, 0);
    assert_eq!(final_read.series.samples.len(), 500);
    assert_eq!(
        final_read
            .series
            .samples
            .last()
            .map(|sample| sample.revision),
        Some(500)
    );

    std::fs::remove_dir_all(&root).expect("remove exact history fixture");
}

#[test]
fn frontend_shutdown_releases_the_unique_writer_before_next_generation() {
    static NEXT_FIXTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let fixture_id = NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp/app-host-frontend-shutdown")
        .join(format!("{}-{fixture_id}", std::process::id()));

    let first =
        HistoryPersistenceCoordinator::start_path(&root, |_| false).expect("first frontend writer");
    assert_eq!(
        first.into_persistence_writer().shutdown(),
        HistoryPersistenceShutdownOutcome::Released
    );
    let second = HistoryPersistenceCoordinator::start_path(&root, |_| false)
        .expect("released writer lock must admit the next generation");
    assert_eq!(
        second.into_persistence_writer().shutdown(),
        HistoryPersistenceShutdownOutcome::Released
    );

    std::fs::remove_dir_all(&root).expect("remove exact frontend shutdown fixture");
}

struct PanickingRecordBackend;

impl HistoryPersistenceBackend for PanickingRecordBackend {
    fn record(&mut self, _command: RecordCommand) -> BackendRecordOutcome {
        panic!("fixture writer record fault");
    }

    fn flush(&mut self, _now_ms: u64) -> Result<(), taskmanager_history_store::HistoryStoreError> {
        Ok(())
    }
}

#[test]
fn backend_panic_flips_the_monitor_off_running_and_keeps_drops_observable() {
    let coordinator = HistoryPersistenceCoordinator::start_with_backend(
        Box::new(PanickingRecordBackend),
        Duration::from_secs(60),
    )
    .expect("start fixture worker");
    let monitor = coordinator.monitor();
    let sink = coordinator.record_sink();
    let key = HistorySeriesKey::system(HistoryMetric::CpuUsagePct);
    let sample = HistoricalSample {
        revision: 1,
        completed_at_ms: 1,
        measured_at_ms: Some(1),
        value: Some(1.0),
    };
    sink.record_sample(key.clone(), sample);

    let deadline = Instant::now() + Duration::from_secs(2);
    let status = loop {
        let status = monitor.status();
        if status.worker != HistoryPersistenceWorkerState::Running {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "worker fault never flipped the monitor"
        );
        std::thread::yield_now();
    };
    assert_eq!(status.worker, HistoryPersistenceWorkerState::Stopped);
    let fault = status.worker_fault.expect("bounded fault detail in health");
    assert!(fault.contains("fixture writer record fault"));

    // A dead write lane must surface later samples as counted drops instead
    // of silently vanishing them behind a healthy-looking monitor.
    sink.record_sample(key, sample);
    let deadline = Instant::now() + Duration::from_secs(2);
    while monitor.status().record_lane_drops == 0 {
        assert!(
            Instant::now() < deadline,
            "post-fault records never surfaced as drops"
        );
        std::thread::yield_now();
    }
    drop(sink);
    drop(coordinator);
}
