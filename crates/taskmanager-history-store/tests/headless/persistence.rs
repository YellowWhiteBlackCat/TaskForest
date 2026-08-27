//! Behavior tests for the persistent history store: JSONL round-trips,
//! in-session revision guarding, windowed queries, clock-jump honesty,
//! corrupt-line tolerance, and both retention trims. All I/O lands in a small
//! unique temp directory removed on exit.

use std::sync::Arc;

use taskmanager_core::{
    DeviceId, HistoricalSample, HistoryMetric, HistoryRecordSink, HistorySeriesKey, HistoryWindow,
};
use taskmanager_history_store::{
    FlushReport, HistoryQuery, HistoryStoreErrorKind, MAX_BOOT_HISTORY_BYTES,
    MAX_DIRECTORY_ENTRIES_PER_SCAN, MAX_PENDING_BYTES, MAX_PENDING_SAMPLES, MAX_PENDING_SERIES,
    MAX_SERIES_FILE_BYTES, MAX_SERIES_FILES, MAX_SERIES_FILES_PER_SCAN, MAX_SERIES_KEY_BYTES,
    MAX_TRACKED_SERIES, PersistentHistoryStore, RecordSampleOutcome, RecordSampleRejection,
    RetentionPolicy, TRIM_INTERVAL_MS, boot_history_path,
};

/// Deterministic liveness probe for tests: the holder is always alive, so
/// only an explicit Drop release ever frees the lock.
fn alive_probe(_pid: u32) -> bool {
    false
}
const ALIVE: fn(u32) -> bool = alive_probe;

fn fixture_root(tag: &str) -> std::path::PathBuf {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "taskmanager-history-store-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn cleanup(root: &std::path::Path) {
    let _ = std::fs::remove_dir_all(root);
}

fn sample(revision: u64, completed_at_ms: u64, value: Option<f64>) -> HistoricalSample {
    HistoricalSample {
        revision,
        completed_at_ms,
        measured_at_ms: (completed_at_ms > 0).then_some(completed_at_ms),
        value,
    }
}

fn cpu_key() -> HistorySeriesKey {
    HistorySeriesKey::system(HistoryMetric::CpuUsagePct)
}

fn record(store: &PersistentHistoryStore, key: &HistorySeriesKey, sample: HistoricalSample) {
    HistoryRecordSink::record_sample(store, key.clone(), sample);
}

#[test]
fn round_trip_preserves_values_gaps_and_order() {
    let root = fixture_root("round-trip");
    let store = PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
        ALIVE,
    )
    .expect("open store");
    let key = cpu_key();
    record(&store, &key, sample(1, 1_000, Some(11.0)));
    record(&store, &key, sample(2, 2_000, None));
    record(&store, &key, sample(3, 3_000, Some(33.0)));

    let report = store.flush(3_000).expect("flush");
    assert_eq!(
        report,
        FlushReport {
            appended_series: 1,
            appended_samples: 3,
            ttl_trimmed_files: 0,
            quota_trimmed_files: 0,
            stale_temporaries_swept: 0,
            temporary_sweep_failures: 0,
        }
    );

    let read = store
        .query()
        .series(&key, HistoryWindow::SevenDays, 3_000)
        .expect("query")
        .expect("the flushed series exists");
    assert_eq!(
        read.series
            .samples
            .iter()
            .map(|sample| (sample.revision, sample.value))
            .collect::<Vec<_>>(),
        vec![(1, Some(11.0)), (2, None), (3, Some(33.0))],
        "values and gaps must survive the JSONL round trip in order"
    );
    assert_eq!(read.series.gap_count(), 1);
    assert_eq!(read.corrupt_lines, 0);
    let status = store.status();
    assert_eq!(status.records_received, 3);
    assert_eq!(status.samples_written, 3);
    drop(store);
    cleanup(&root);
}

#[test]
fn in_session_duplicates_are_dropped_and_new_sessions_append() {
    let root = fixture_root("sessions");
    let policy = RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX);
    let key = cpu_key();
    {
        let store = PersistentHistoryStore::open(&root, policy, ALIVE).expect("open store");
        record(&store, &key, sample(5, 5_000, Some(50.0)));
        // Same-session replay of an accepted revision: defense in depth.
        record(&store, &key, sample(5, 5_000, Some(99.0)));
        store.flush(5_000).expect("flush");
        assert_eq!(store.status().duplicate_records_dropped, 1);
    }
    // A fresh process legitimately restarts revisions at 1; the append-only
    // file keeps both runs in chronological order.
    let store = PersistentHistoryStore::open(&root, policy, ALIVE).expect("reopen store");
    record(&store, &key, sample(1, 50_000, Some(10.0)));
    store.flush(50_000).expect("flush");

    let read = store
        .query()
        .series(&key, HistoryWindow::SevenDays, 60_000)
        .expect("query")
        .expect("series exists");
    assert_eq!(
        read.series
            .samples
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        vec![Some(50.0), Some(10.0)],
        "the new session's restart revision must append, not overwrite"
    );
    drop(store);
    cleanup(&root);
}

#[test]
fn window_queries_filter_by_completion_time() {
    let root = fixture_root("windows");
    // TTL disabled: this test exercises the QUERY window, not retention.
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(u64::MAX, u64::MAX), ALIVE)
            .expect("open store");
    let key = cpu_key();
    let hour_ms = HistoryWindow::OneHour.duration_ms();
    let now = 10 * hour_ms;
    record(&store, &key, sample(1, now - hour_ms - 5_000, Some(1.0)));
    record(&store, &key, sample(2, now - hour_ms + 5_000, Some(2.0)));
    store.flush(now).expect("flush");

    let query = store.query();
    let one_hour = query
        .series(&key, HistoryWindow::OneHour, now)
        .expect("query")
        .expect("series exists");
    assert_eq!(
        one_hour.series.samples.len(),
        1,
        "only the sample inside the window may appear"
    );
    assert_eq!(one_hour.series.samples[0].value, Some(2.0));
    let seven_days = query
        .series(&key, HistoryWindow::SevenDays, now)
        .expect("query")
        .expect("series exists");
    assert_eq!(seven_days.series.samples.len(), 2);
    assert!(
        query
            .series(
                &HistorySeriesKey::system(HistoryMetric::SwapUsedPct),
                HistoryWindow::SevenDays,
                now
            )
            .expect("query")
            .is_none(),
        "a series with no file must be Ok(None), not an error"
    );
    drop(store);
    cleanup(&root);
}

#[test]
fn backward_clock_steps_stay_recorded_and_surfaced() {
    let root = fixture_root("clock-jump");
    let store = PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
        ALIVE,
    )
    .expect("open store");
    let key = cpu_key();
    record(&store, &key, sample(1, 9_000_000, Some(40.0)));
    record(&store, &key, sample(2, 8_900_000, Some(41.0)));
    store.flush(9_000_000).expect("flush");

    let read = store
        .query()
        .series(&key, HistoryWindow::SevenDays, 9_000_000)
        .expect("query")
        .expect("series exists");
    assert_eq!(read.series.samples.len(), 2, "jumped samples are kept");
    assert_eq!(read.series.clock_jumps, 1, "the step backwards is counted");
    drop(store);
    cleanup(&root);
}

#[test]
fn peak_summary_reports_facts_only() {
    let root = fixture_root("peak");
    let store = PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
        ALIVE,
    )
    .expect("open store");
    let key = cpu_key();
    record(&store, &key, sample(1, 1_000, Some(41.0)));
    record(&store, &key, sample(2, 2_000, None));
    record(&store, &key, sample(3, 3_000, Some(87.5)));
    store.flush(3_000).expect("flush");

    let summary = store
        .query()
        .peak_summary(&key, HistoryWindow::SevenDays, 3_000)
        .expect("query")
        .expect("series exists");
    assert_eq!(summary.peak_value, Some(87.5));
    assert_eq!(summary.peak_measured_at_ms, Some(3_000));
    assert_eq!(summary.observed_samples, 2);
    assert_eq!(summary.gap_samples, 1);
    assert_eq!(summary.clock_jumps, 0);
    drop(store);
    cleanup(&root);
}

#[test]
fn corrupt_lines_are_skipped_and_counted() {
    let root = fixture_root("corrupt");
    let key = cpu_key();
    let store = PersistentHistoryStore::open(
        &root,
        RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
        ALIVE,
    )
    .expect("open store");
    record(&store, &key, sample(1, 1_000, Some(10.0)));
    store.flush(1_000).expect("flush");
    drop(store);

    // Damage the file like a torn write would.
    let path = root.join(format!("{}.jsonl", key.file_stem()));
    let text = std::fs::read_to_string(&path).expect("read series file");
    std::fs::write(&path, format!("{text}{{torn write\n")).expect("damage series file");

    let query = HistoryQuery::new(&root);
    let read = query
        .series(&key, HistoryWindow::SevenDays, 2_000)
        .expect("query")
        .expect("series exists");
    assert_eq!(read.corrupt_lines, 1);
    assert_eq!(read.series.samples.len(), 1);
    assert_eq!(
        query.known_series().expect("list series"),
        vec![key],
        "a corrupt tail must not hide the series"
    );
    cleanup(&root);
}

#[test]
fn ttl_trim_drops_expired_samples_but_keeps_future_dated_ones() {
    let root = fixture_root("ttl");
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(5_000, u64::MAX), ALIVE)
            .expect("open store");
    let key = cpu_key();
    record(&store, &key, sample(1, 1_000, Some(1.0)));
    record(&store, &key, sample(2, 4_000, Some(2.0)));
    record(&store, &key, sample(3, 6_000, Some(3.0)));
    // A clock step backwards recorded a "future" completion time: expiry by
    // age must not apply to it.
    record(&store, &key, sample(4, 20_000, Some(4.0)));

    let report = store.flush(10_000).expect("flush with trim");
    assert_eq!(report.ttl_trimmed_files, 1);

    let read = store
        .query()
        .series(&key, HistoryWindow::SevenDays, 10_000)
        .expect("query")
        .expect("series exists");
    let kept_times: Vec<u64> = read
        .series
        .samples
        .iter()
        .map(|sample| sample.completed_at_ms)
        .collect();
    assert_eq!(kept_times, vec![6_000, 20_000]);
    drop(store);
    cleanup(&root);
}

#[test]
fn ttl_retires_empty_series_and_releases_its_in_session_revision_guard() {
    let root = fixture_root("ttl-retire-series");
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(5_000, u64::MAX), ALIVE)
            .expect("open store");
    let key = cpu_key();
    record(&store, &key, sample(1, 1_000, Some(1.0)));
    store.flush(10_000).expect("expire the only sample");
    assert!(
        store
            .query()
            .series(&key, HistoryWindow::SevenDays, 10_000)
            .expect("query retired series")
            .is_none(),
        "a fully expired series must not survive as an empty file"
    );
    assert!(
        store
            .query()
            .known_series()
            .expect("list series")
            .is_empty()
    );

    // Retirement ends the old in-session series lifetime. Reusing the same
    // identity begins a fresh revision guard instead of being dropped as a
    // duplicate of data that no longer exists.
    record(&store, &key, sample(1, 11_000, Some(2.0)));
    store.flush(11_000).expect("recreate retired series");
    let read = store
        .query()
        .series(&key, HistoryWindow::SevenDays, 11_000)
        .expect("query recreated series")
        .expect("recreated series exists");
    assert_eq!(read.series.samples, vec![sample(1, 11_000, Some(2.0))]);
    drop(store);
    cleanup(&root);
}

#[test]
fn quota_retires_whole_series_when_minimum_records_cannot_fit() {
    let root = fixture_root("quota");
    let store = PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(u64::MAX, 1), ALIVE)
        .expect("open store");
    let oldest = HistorySeriesKey::system(HistoryMetric::CpuUsagePct);
    let newest = HistorySeriesKey::system(HistoryMetric::MemoryUsedPct);
    for revision in 1..=4 {
        record(
            &store,
            &oldest,
            sample(revision, revision * 1_000, Some(revision as f64)),
        );
    }
    for revision in 1..=4 {
        record(
            &store,
            &newest,
            sample(revision, 100_000 + revision * 1_000, Some(revision as f64)),
        );
    }

    let report = store.flush(200_000).expect("flush with trim");
    assert!(report.quota_trimmed_files >= 1, "a 1-byte quota must trim");

    // A one-byte quota cannot hold even one honest JSON record. Retention
    // retires whole old series instead of leaving unbounded one-line files.
    let known = store.query().known_series().expect("list series");
    assert!(known.is_empty());

    // Retirement releases the in-session guard. The same logical series can
    // start a fresh lifetime at revision 1 instead of being rejected as a
    // duplicate of data that no longer exists.
    assert_eq!(
        store.try_record_sample(oldest, sample(1, 201_000, Some(9.0))),
        RecordSampleOutcome::Accepted
    );
    drop(store);
    cleanup(&root);
}

#[test]
fn dropping_the_store_flushes_its_pending_samples() {
    let root = fixture_root("drop-flush");
    let key = cpu_key();
    {
        let store = PersistentHistoryStore::open(
            &root,
            RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
            ALIVE,
        )
        .expect("open store");
        record(&store, &key, sample(1, 7_000, Some(70.0)));
        // No explicit flush: Drop must use the last seen completion time.
    }

    let read = HistoryQuery::new(&root)
        .series(&key, HistoryWindow::SevenDays, 8_000)
        .expect("query")
        .expect("drop flushed the samples");
    assert_eq!(read.series.samples.len(), 1);
    assert_eq!(read.series.samples[0].value, Some(70.0));
    cleanup(&root);
}

#[test]
fn shared_as_sink_trait_object_records_normally() {
    let root = fixture_root("arc-sink");
    let store = Arc::new(
        PersistentHistoryStore::open(
            &root,
            RetentionPolicy::for_tests(TRIM_INTERVAL_MS, u64::MAX),
            ALIVE,
        )
        .expect("open store"),
    );
    let sink: Arc<dyn HistoryRecordSink> = store.clone();
    sink.record_sample(cpu_key(), sample(1, 1_000, Some(5.0)));
    store.flush(1_000).expect("flush");
    assert_eq!(store.status().samples_written, 1);
    drop(store);
    cleanup(&root);
}

#[test]
fn boot_history_deduplicates_content_and_returns_the_previous_baseline() {
    use taskmanager_core::{BootTimeline, BootTimelineSegment};
    use taskmanager_history_store::{BootEvidenceHistory, RecordBootOutcome};

    fn timeline(total_ms: u64, units: &[(&str, u64)]) -> BootTimeline {
        BootTimeline {
            total_ms,
            segments: units
                .iter()
                .map(|(unit, duration)| BootTimelineSegment {
                    unit: (*unit).to_owned(),
                    start_ms: 0,
                    end_ms: *duration,
                    duration_ms: *duration,
                })
                .collect(),
            collapsed_count: 0,
            untimed_count: 0,
            untimed_units: Vec::new(),
        }
    }

    let root = fixture_root("boots");
    let history = BootEvidenceHistory::new(&root);

    // First recorded boot ever: appended, no baseline.
    let first = timeline(900, &[("dev-node.service", 900)]);
    assert_eq!(
        history.record_boot(&first, 1_000),
        Ok(RecordBootOutcome::NewBoot { previous: None })
    );
    // Same boot redelivered: no append, still no baseline.
    assert_eq!(
        history.record_boot(&first, 2_000),
        Ok(RecordBootOutcome::SameBoot { previous: None })
    );

    // A new boot: appended, baseline = the previous boot.
    let second = timeline(800, &[("dev-node.service", 800)]);
    assert_eq!(
        history.record_boot(&second, 10_000),
        Ok(RecordBootOutcome::NewBoot {
            previous: Some(first.clone())
        })
    );
    // Same-boot redelivery now resolves the baseline = the boot before last.
    assert_eq!(
        history.record_boot(&second, 11_000),
        Ok(RecordBootOutcome::SameBoot {
            previous: Some(first.clone())
        })
    );
    assert_eq!(history.boots().expect("read boots").len(), 2);

    // The bound keeps the newest boots.
    for index in 0..10 {
        let boot = timeline(index, &[("dev-node.service", index)]);
        history.record_boot(&boot, 20_000 + index).expect("record");
    }
    let boots = history.boots().expect("read boots");
    assert_eq!(boots.len(), taskmanager_history_store::MAX_RECORDED_BOOTS);
    assert_eq!(boots.last().expect("newest").timeline.total_ms, 9);

    drop(history);
    cleanup(&root);
}

#[test]
fn boot_history_treats_a_completing_chain_as_the_same_boot() {
    use taskmanager_core::{BootTimeline, BootTimelineSegment};
    use taskmanager_history_store::{BootEvidenceHistory, RecordBootOutcome};

    fn timeline(units: &[(&str, u64)]) -> BootTimeline {
        BootTimeline {
            total_ms: units.iter().map(|(_, d)| *d).max().unwrap_or(0),
            segments: units
                .iter()
                .map(|(unit, duration)| BootTimelineSegment {
                    unit: (*unit).to_owned(),
                    start_ms: 0,
                    end_ms: *duration,
                    duration_ms: *duration,
                })
                .collect(),
            collapsed_count: 0,
            untimed_count: 0,
            untimed_units: Vec::new(),
        }
    }

    let root = fixture_root("boot-evolution");
    let history = BootEvidenceHistory::new(&root);

    // A provider retry first reports one unit, then the complete chain.
    let partial = timeline(&[("dev-node.service", 900)]);
    let complete = timeline(&[("dev-node.service", 900), ("network.service", 300)]);
    history.record_boot(&partial, 1_000).expect("partial");
    assert_eq!(
        history.record_boot(&complete, 2_000),
        Ok(RecordBootOutcome::SameBoot { previous: None }),
        "a completing chain is the SAME boot, not a phantom new one"
    );
    // The record was updated in place: still one boot, complete chain.
    let boots = history.boots().expect("read boots");
    assert_eq!(boots.len(), 1);
    assert_eq!(boots[0].timeline.segments.len(), 2);
    assert_eq!(boots[0].recorded_at_ms, 1_000, "first-seen time is kept");

    // A genuinely different boot (changed timing) still appends.
    let next_boot = timeline(&[("dev-node.service", 700), ("network.service", 300)]);
    assert_eq!(
        history.record_boot(&next_boot, 90_000),
        Ok(RecordBootOutcome::NewBoot {
            previous: Some(complete)
        })
    );
    drop(history);
    cleanup(&root);
}

#[test]
fn quota_trim_converges_under_sustained_pressure_and_keeps_every_series() {
    use taskmanager_history_store::{PersistentHistoryStore, RetentionPolicy, TRIM_INTERVAL_MS};

    let root = fixture_root("quota-pressure");
    // Tiny quota: every flush's trim pass must fight to fit.
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(u64::MAX, 4_096), ALIVE)
            .expect("open store");
    let keys = [
        HistorySeriesKey::system(HistoryMetric::CpuUsagePct),
        HistorySeriesKey::system(HistoryMetric::MemoryUsedPct),
        HistorySeriesKey::system(HistoryMetric::SwapUsedPct),
    ];
    let mut now = 100_000u64;
    for round in 0..12u64 {
        for (series, key) in keys.iter().enumerate() {
            for sample in 0..8u64 {
                record(
                    &store,
                    key,
                    HistoricalSample {
                        revision: round * 24 + (series as u64) * 8 + sample + 1,
                        completed_at_ms: now,
                        measured_at_ms: Some(now),
                        value: Some((series as f64) + sample as f64),
                    },
                );
            }
            now += 1_000;
        }
        // Advance past the trim interval so every flush trims.
        now += TRIM_INTERVAL_MS;
        let _ = store.flush(now);
    }

    // Convergence is exact: if minimum one-line files do not fit, the oldest
    // complete series is retired.
    let total: u64 = std::fs::read_dir(&root)
        .expect("list history dir")
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum();
    assert!(
        total <= 4_096,
        "quota must bound the directory (got {total} bytes)"
    );
    // Every series survived the pressure with samples at all.
    for key in &keys {
        let read = store
            .query()
            .series(key, HistoryWindow::SevenDays, now)
            .expect("query under pressure")
            .expect("series file must survive quota pressure");
        assert!(
            !read.series.samples.is_empty(),
            "series {:?} must stay alive under quota pressure",
            key.metric()
        );
    }
    drop(store);
    cleanup(&root);
}

#[test]
fn pending_samples_are_globally_bounded_and_keep_the_newest_arrivals() {
    let root = fixture_root("pending-sample-bound");
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(u64::MAX, u64::MAX), ALIVE)
            .expect("open store");
    let key = cpu_key();
    let total = MAX_PENDING_SAMPLES + 2;
    for revision in 1..=total {
        let revision = u64::try_from(revision).expect("test revision fits u64");
        let _ = store.try_record_sample(key.clone(), sample(revision, revision, Some(1.0)));
    }

    let status = store.status();
    assert_eq!(status.pending_samples, MAX_PENDING_SAMPLES);
    assert!(status.pending_bytes <= MAX_PENDING_BYTES);
    assert_eq!(status.samples_dropped_backpressure, 2);
    store
        .flush(u64::try_from(total).expect("test time fits u64"))
        .expect("flush bounded buffer");
    let read = store
        .query()
        .series(
            &key,
            HistoryWindow::SevenDays,
            u64::try_from(total).expect("test time fits u64"),
        )
        .expect("query")
        .expect("series exists");
    assert_eq!(read.series.samples.len(), MAX_PENDING_SAMPLES);
    assert_eq!(
        read.series.samples.first().map(|sample| sample.revision),
        Some(3)
    );
    drop(store);
    cleanup(&root);
}

#[test]
fn failed_flush_requeue_preserves_global_bounds_and_can_retry_in_order() {
    let root = fixture_root("failed-flush-requeue-bound");
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(u64::MAX, u64::MAX), ALIVE)
            .expect("open store");
    let key = cpu_key();
    let total = MAX_PENDING_SAMPLES + 2;
    for revision in 1..=total {
        let revision = u64::try_from(revision).expect("test revision fits u64");
        record(&store, &key, sample(revision, revision, Some(1.0)));
    }
    let before = store.status();

    std::fs::remove_dir_all(&root).expect("remove backing directory");
    std::fs::write(&root, b"not a directory").expect("block backing path");
    let error = store.flush(10_000).expect_err("append must fail");
    assert_eq!(error.kind(), HistoryStoreErrorKind::Read);
    let requeued = store.status();
    assert_eq!(requeued.pending_samples, MAX_PENDING_SAMPLES);
    assert_eq!(requeued.pending_bytes, before.pending_bytes);
    assert_eq!(
        requeued.samples_dropped_backpressure,
        before.samples_dropped_backpressure
    );

    std::fs::remove_file(&root).expect("unblock backing path");
    std::fs::create_dir_all(&root).expect("restore backing directory");
    store.flush(10_001).expect("retry bounded queue");
    let read = store
        .query()
        .series(&key, HistoryWindow::SevenDays, 10_001)
        .expect("query")
        .expect("series exists");
    assert_eq!(read.series.samples.len(), MAX_PENDING_SAMPLES);
    assert_eq!(
        read.series.samples.first().map(|sample| sample.revision),
        Some(3)
    );
    drop(store);
    cleanup(&root);
}

#[test]
fn pending_payload_bytes_evict_old_unpersisted_series_without_leaking_guards() {
    let root = fixture_root("pending-byte-bound");
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(u64::MAX, u64::MAX), ALIVE)
            .expect("open store");
    for index in 0..1_500usize {
        let identity = format!("{index:04}-{}", "x".repeat(3_400));
        let key = HistorySeriesKey::for_device(HistoryMetric::GpuUsagePct, DeviceId::new(identity));
        let outcome = store.try_record_sample(key, sample(1, 0, Some(1.0)));
        assert!(
            matches!(
                outcome,
                RecordSampleOutcome::Accepted
                    | RecordSampleOutcome::AcceptedWithBackpressure { .. }
            ),
            "retiring an unpersisted oldest series must reopen its tracking slot"
        );
    }

    let status = store.status();
    assert!(status.pending_bytes <= MAX_PENDING_BYTES);
    assert!(status.pending_series <= MAX_PENDING_SERIES);
    assert_eq!(status.pending_samples, status.pending_series);
    assert_eq!(status.tracked_series, status.pending_series);
    assert!(status.samples_dropped_backpressure > 0);
    drop(store);
    cleanup(&root);
}

#[test]
fn tracked_series_and_identity_size_have_typed_admission_limits() {
    let root = fixture_root("tracked-series-bound");
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(u64::MAX, u64::MAX), ALIVE)
            .expect("open store");
    for index in 0..MAX_TRACKED_SERIES {
        let key = HistorySeriesKey::for_device(
            HistoryMetric::GpuUsagePct,
            DeviceId::new(format!("gpu-{index}")),
        );
        assert_eq!(
            store.try_record_sample(key, sample(1, 0, Some(1.0))),
            RecordSampleOutcome::Accepted
        );
    }
    let overflow = HistorySeriesKey::for_device(
        HistoryMetric::GpuUsagePct,
        DeviceId::new("one-series-too-many"),
    );
    assert_eq!(
        store.try_record_sample(overflow, sample(1, 0, Some(1.0))),
        RecordSampleOutcome::Rejected(RecordSampleRejection::TrackedSeriesLimit {
            max_series: MAX_TRACKED_SERIES,
        })
    );
    let oversized = HistorySeriesKey::for_device(
        HistoryMetric::GpuUsagePct,
        DeviceId::new("z".repeat(MAX_SERIES_KEY_BYTES + 1)),
    );
    assert!(matches!(
        store.try_record_sample(oversized, sample(1, 0, Some(1.0))),
        RecordSampleOutcome::Rejected(RecordSampleRejection::SeriesKeyTooLong { .. })
    ));
    let status = store.status();
    assert_eq!(status.tracked_series, MAX_TRACKED_SERIES);
    assert_eq!(status.pending_series, MAX_PENDING_SERIES);
    assert_eq!(status.samples_rejected_resource_limit, 2);
    drop(store);
    cleanup(&root);
}

#[test]
fn invalid_metric_scope_is_rejected_before_it_can_create_a_series_file() {
    let root = fixture_root("invalid-series-scope");
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(u64::MAX, u64::MAX), ALIVE)
            .expect("open store");
    let invalid = HistorySeriesKey::system(HistoryMetric::ApplicationCpuUsagePct);
    assert_eq!(
        store.try_record_sample(invalid, sample(1, 0, Some(1.0))),
        RecordSampleOutcome::Rejected(RecordSampleRejection::InvalidSeriesScope)
    );
    assert_eq!(store.status().tracked_series, 0);
    drop(store);
    cleanup(&root);
}

#[test]
fn query_refuses_oversized_series_and_retention_retires_it_without_reading_it() {
    let root = fixture_root("oversized-series-file");
    std::fs::create_dir_all(&root).expect("create root");
    let key = cpu_key();
    let path = root.join(format!("{}.jsonl", key.file_stem()));
    let file = std::fs::File::create(&path).expect("create sparse series");
    file.set_len(MAX_SERIES_FILE_BYTES + 1)
        .expect("enlarge sparse series");
    drop(file);

    let error = HistoryQuery::new(&root)
        .series(&key, HistoryWindow::SevenDays, 1)
        .expect_err("oversized query must fail closed");
    assert_eq!(error.kind(), HistoryStoreErrorKind::ResourceLimit);

    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(u64::MAX, u64::MAX), ALIVE)
            .expect("open store");
    let report = store.flush(1).expect("retire oversized series");
    assert_eq!(report.quota_trimmed_files, 1);
    assert!(
        store
            .query()
            .series(&key, HistoryWindow::SevenDays, 1)
            .expect("query retired file")
            .is_none()
    );
    drop(store);
    cleanup(&root);
}

#[test]
fn series_file_count_is_fail_closed_for_queries_and_reconciled_by_retention() {
    let root = fixture_root("series-file-count");
    std::fs::create_dir_all(&root).expect("create root");
    for index in 0..=MAX_SERIES_FILES {
        let key = HistorySeriesKey::for_device(
            HistoryMetric::GpuUsagePct,
            DeviceId::new(format!("gpu-{index}")),
        );
        std::fs::write(
            root.join(format!("{}.jsonl", key.file_stem())),
            format!("{{\"r\":1,\"c\":{},\"m\":null,\"v\":1.0}}\n", index + 1),
        )
        .expect("write series fixture");
    }
    let error = HistoryQuery::new(&root)
        .known_series()
        .expect_err("unbounded enumeration must fail closed");
    assert_eq!(error.kind(), HistoryStoreErrorKind::ResourceLimit);

    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(u64::MAX, u64::MAX), ALIVE)
            .expect("open store");
    let report = store.flush(2_000).expect("reconcile file count");
    assert_eq!(report.quota_trimmed_files, 1);
    assert_eq!(
        store.query().known_series().expect("bounded listing").len(),
        MAX_SERIES_FILES
    );
    drop(store);
    cleanup(&root);
}

#[test]
fn boot_history_read_has_a_typed_byte_limit() {
    use taskmanager_history_store::BootEvidenceHistory;

    let root = fixture_root("oversized-boot-history");
    std::fs::create_dir_all(&root).expect("create root");
    let path = boot_history_path(&root);
    let file = std::fs::File::create(&path).expect("create sparse boot history");
    file.set_len(MAX_BOOT_HISTORY_BYTES + 1)
        .expect("enlarge sparse boot history");
    drop(file);

    let error = BootEvidenceHistory::new(&root)
        .boots()
        .expect_err("oversized boot history must fail closed");
    assert_eq!(error.kind(), HistoryStoreErrorKind::ResourceLimit);
    cleanup(&root);
}

#[test]
fn retention_scan_limit_fails_before_mutating_an_external_file_flood() {
    let root = fixture_root("retention-scan-bound");
    std::fs::create_dir_all(&root).expect("create root");
    for index in 0..=MAX_SERIES_FILES_PER_SCAN {
        std::fs::write(root.join(format!("external-{index}.jsonl")), b"corrupt\n")
            .expect("write external fixture");
    }
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(u64::MAX, u64::MAX), ALIVE)
            .expect("open store");
    let error = store
        .flush(1)
        .expect_err("an external file flood must not monopolize retention");
    assert_eq!(error.kind(), HistoryStoreErrorKind::ResourceLimit);
    let remaining = std::fs::read_dir(&root)
        .expect("list root")
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .count();
    assert_eq!(
        remaining,
        MAX_SERIES_FILES_PER_SCAN + 1,
        "fail-closed preflight must not partially retire an unscanned directory"
    );
    drop(store);
    cleanup(&root);
}

#[test]
fn mixed_non_series_entry_flood_is_bounded_for_query_and_retention() {
    let root = fixture_root("mixed-directory-entry-bound");
    std::fs::create_dir_all(&root).expect("create root");
    for index in 0..=MAX_DIRECTORY_ENTRIES_PER_SCAN {
        std::fs::write(root.join(format!("external-{index}.junk")), b"debris")
            .expect("write external debris");
    }
    let query_error = HistoryQuery::new(&root)
        .known_series()
        .expect_err("query directory walk must have a total entry bound");
    assert_eq!(query_error.kind(), HistoryStoreErrorKind::ResourceLimit);

    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(u64::MAX, u64::MAX), ALIVE)
            .expect("open store");
    let flush_error = store
        .flush(1)
        .expect_err("retention directory walk must have a total entry bound");
    assert_eq!(flush_error.kind(), HistoryStoreErrorKind::ResourceLimit);
    let debris = std::fs::read_dir(&root)
        .expect("list root")
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "junk")
        })
        .count();
    assert_eq!(debris, MAX_DIRECTORY_ENTRIES_PER_SCAN + 1);
    drop(store);
    cleanup(&root);
}
