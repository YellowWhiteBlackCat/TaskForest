//! Read-mostly history contention and lock-latency regression.

use super::*;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use taskmanager_core::{HistoricalSample, HistoryRecordSink, HistorySeriesKey};

/// Persistence probe that stops revision 1 after its fixed CPU rings were
/// written but before per-core fan-out and the public commit marker. That
/// gives revision 2 a deterministic chance to overtake it if the domain
/// transaction guard is ever removed.
struct BlockingCommitSink {
    blocked_once: AtomicBool,
    entered: mpsc::SyncSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    records: Mutex<Vec<(HistorySeriesKey, HistoricalSample)>>,
}

impl HistoryRecordSink for BlockingCommitSink {
    fn record_sample(&self, key: HistorySeriesKey, sample: HistoricalSample) {
        if sample.revision == 1 && !self.blocked_once.swap(true, Ordering::AcqRel) {
            self.entered
                .send(())
                .expect("transaction test still owns the entered receiver");
            self.release
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .recv()
                .expect("transaction test releases the first writer");
        }
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((key, sample));
    }
}

fn transaction_cpu(value: f32, observed_at_ms: u64) -> CpuTelemetryObservation {
    CpuTelemetryObservation::current(
        CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(value, observed_at_ms),
            core_usage_group: available_group([value], observed_at_ms),
            ..Default::default()
        }),
        observed_at_ms,
        Vec::new(),
    )
}

/// Same-domain acceptance, every ring fan-out, persistence, receipts and the
/// public revision are one commit. In particular, a higher revision cannot
/// enter while revision 1 is paused in its sink, and a reader cannot observe
/// the already-written fixed ring under the still-old public revision.
#[test]
fn same_domain_writers_commit_in_order_and_hide_in_flight_watermarks() {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let sink = Arc::new(BlockingCommitSink {
        blocked_once: AtomicBool::new(false),
        entered: entered_tx,
        release: Mutex::new(release_rx),
        records: Mutex::new(Vec::new()),
    });
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(8);
    let ingestor = ingestor.with_record_sink(sink.clone());

    let first_ingestor = ingestor.clone();
    let first = thread::spawn(move || {
        first_ingestor
            .ingest_correlated_cpu(stamp_at(1, 100), &transaction_cpu(10.0, 100))
            .expect("first CPU transaction commits");
    });
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("first writer reaches the controlled persistence boundary");
    assert_eq!(
        store.system_history.revision(),
        0,
        "the public generation must remain old until every fan-out and sink write commits"
    );

    let (read_started_tx, read_started_rx) = mpsc::sync_channel(1);
    let (read_done_tx, read_done_rx) = mpsc::sync_channel(1);
    let read_store = store.clone();
    let reader = thread::spawn(move || {
        read_started_tx.send(()).expect("reader start handshake");
        let watermark = read_store.system_history.cpu_usage().watermark();
        let revision = read_store.system_history.revision();
        read_done_tx
            .send((watermark, revision))
            .expect("reader result receiver remains live");
    });
    read_started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("reader thread started");

    let (second_started_tx, second_started_rx) = mpsc::sync_channel(1);
    let (second_done_tx, second_done_rx) = mpsc::sync_channel(1);
    let second_ingestor = ingestor.clone();
    let second = thread::spawn(move || {
        second_started_tx
            .send(())
            .expect("second writer start handshake");
        second_ingestor
            .ingest_correlated_cpu(stamp_at(2, 200), &transaction_cpu(20.0, 200))
            .expect("second CPU transaction commits");
        second_done_tx
            .send(())
            .expect("second writer result receiver remains live");
    });
    second_started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("second writer thread started");
    assert!(
        second_done_rx
            .recv_timeout(Duration::from_millis(100))
            .is_err(),
        "a same-domain writer must wait behind the in-flight commit"
    );

    release_tx
        .send(())
        .expect("release the controlled first writer");
    first.join().expect("first writer thread joins");
    second.join().expect("second writer thread joins");
    reader.join().expect("watermark reader thread joins");

    let (watermark, revision_at_read) = read_done_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("reader returns after a complete commit");
    assert!(
        revision_at_read >= 1,
        "a visible ring watermark must never pair with the pre-commit generation"
    );
    assert!(watermark.0 >= 1 && watermark.1.is_some());
    assert_eq!(store.system_history.revision(), 2);

    let revisions = |samples: Vec<CorrelatedMetricSample<f32>>| {
        samples
            .into_iter()
            .map(|sample| sample.stamp.revision())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        revisions(store.system_history.cpu_usage().samples()),
        [1, 2]
    );
    assert_eq!(
        revisions(store.system_history.cpu_core_usage()[0].samples()),
        [1, 2],
        "the per-core tail cannot be overtaken while revision 1 is paused in persistence"
    );
    assert_eq!(
        store
            .system_history
            .receipts(SystemHistoryDomain::Cpu)
            .into_iter()
            .map(|receipt| receipt.stamp.revision())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        sink.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .filter(|(key, _)| key.metric() == taskmanager_core::HistoryMetric::CpuUsagePct)
            .map(|(_, sample)| sample.revision)
            .collect::<Vec<_>>(),
        [1, 2],
        "the persistence mirror shares the domain commit order"
    );
}

/// P1-ARCH-02 profiling closure: record short `Mutex` hold latencies under a
/// realistic read-mostly dashboard workload. Four readers spin on
/// `cpu_usage().samples()` (the per-frame chart read) while one writer pushes
/// correlated ticks (the collector lane). The ring buffer's mutex is held for
/// microseconds, so per-call latency in debug mode should stay far below frame
/// budget; the ceilings are deliberately loose to absorb scheduler jitter while
/// still catching a redesign that holds the lock across allocation/copying.
#[test]
fn concurrent_readers_and_writer_profile_short_lock_hold_times() {
    const TICKS: u64 = 500;
    const READERS: usize = 2;
    /// Per-call read latency ceiling (debug-build, loaded CI allowance).
    const READ_LATENCY_LIMIT: Duration = Duration::from_millis(50);
    /// Per-tick write latency ceiling (same allowance).
    const WRITE_LATENCY_LIMIT: Duration = Duration::from_millis(50);

    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(600);
    let stop = Arc::new(AtomicBool::new(false));
    let mut readers = Vec::new();
    for _ in 0..READERS {
        let store = store.clone();
        let stop = stop.clone();
        readers.push(thread::spawn(move || {
            let mut max_latency = Duration::ZERO;
            let mut calls = 0_u64;
            let mut last_len = 0_usize;
            while !stop.load(Ordering::Relaxed) {
                let started = Instant::now();
                let samples = store.system_history.cpu_usage().samples();
                let latency = started.elapsed();
                max_latency = max_latency.max(latency);
                calls += 1;
                // Ring buffers must never present a shrunk or reordered view:
                // the sample count is monotonic until the buffer wraps.
                assert!(
                    samples.len() >= last_len,
                    "reader observed history shrink: {last_len} -> {}",
                    samples.len()
                );
                last_len = samples.len();
                // Dashboard reads happen per frame (~200ms), not in a hot spin.
                thread::sleep(Duration::from_millis(1));
            }
            (calls, max_latency)
        }));
    }

    let writer_started = Instant::now();
    let mut max_write_latency = Duration::ZERO;
    for revision in 1..=TICKS {
        let started = Instant::now();
        let observed_at_ms = revision.saturating_mul(200);
        let observation = CpuTelemetryObservation::current(
            observed_cpu((revision % 100) as f32, observed_at_ms),
            observed_at_ms,
            Vec::new(),
        );
        ingestor
            .ingest_correlated_cpu(
                CorrelatedTelemetryStamp::from_accepted_event(revision, observed_at_ms + 10)
                    .expect("non-zero revisions"),
                &observation,
            )
            .expect("increasing revisions ingest");
        max_write_latency = max_write_latency.max(started.elapsed());
        // The collector ticks on a cadence, not in a blitz: 1ms pacing keeps
        // the writer interleaved with the readers so the profile has real
        // contention samples instead of a one-sided sprint.
        thread::sleep(Duration::from_millis(1));
    }
    let writer_wall = writer_started.elapsed();
    stop.store(true, Ordering::Relaxed);

    let mut total_calls = 0_u64;
    let mut max_read_latency = Duration::ZERO;
    for reader in readers {
        let (calls, latency) = reader.join().expect("reader thread joins");
        total_calls += calls;
        max_read_latency = max_read_latency.max(latency);
    }
    assert_eq!(
        store.system_history.cpu_usage().samples().len(),
        TICKS as usize,
        "every writer tick must land in the bounded history"
    );
    eprintln!(
        "lock profile: {READERS} readers × {total_calls} reads (max {max_read_latency:?}), \
         writer {TICKS} ticks in {writer_wall:?} (max {max_write_latency:?})"
    );
    assert!(
        max_read_latency <= READ_LATENCY_LIMIT,
        "chart read under contention peaked at {max_read_latency:?}, exceeding the \
         {READ_LATENCY_LIMIT:?} ceiling — lock held too long (allocation/copy under lock?)"
    );
    assert!(
        max_write_latency <= WRITE_LATENCY_LIMIT,
        "collector tick peaked at {max_write_latency:?}, exceeding the {WRITE_LATENCY_LIMIT:?} \
         ceiling — lock held too long"
    );
}
