//! test-intent: behavior
//! Ordinary portable files and private external-name ingress over real storage.

use super::*;
use taskmanager_core::core::history::ApplicationHistoryIdentity;

#[test]
fn application_records_create_visible_ordinary_files_and_query_by_the_same_key() {
    let root = fixture_root("portable-applications");
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(10_000, u64::MAX), ALIVE)
            .expect("writer");
    let keys = ["App", "app", "_edge_"].map(|name| {
        HistorySeriesKey::for_application(
            HistoryMetric::ApplicationCpuUsagePct,
            ApplicationHistoryIdentity::verified_launcher(name).expect("identity"),
        )
    });
    for (index, key) in keys.iter().enumerate() {
        record(&store, key, sample(1, 1000, Some(13.0 + index as f64)));
    }
    store.flush(1000).expect("flush portable names");
    let query = HistoryQuery::new(&root);
    let known = query.known_series().expect("ordinary directory entries");
    assert_eq!(
        known.len(),
        3,
        "all identities are ordinary enumerable files, never NTFS alternate streams"
    );
    for (index, key) in keys.iter().enumerate() {
        assert!(known.contains(key));
        let read = query
            .series(key, HistoryWindow::OneHour, 1000)
            .expect("query")
            .expect("recorded file");
        assert_eq!(
            read.series.peak().expect("peak").value,
            Some(13.0 + index as f64)
        );
    }
    drop(store);
    cleanup(&root);
}

#[test]
fn external_spelling_and_current_segments_join_once_without_mutating_the_archive() {
    let root = fixture_root("external-namespace");
    std::fs::create_dir_all(&root).expect("directory");
    let key = HistorySeriesKey::for_application(
        HistoryMetric::ApplicationCpuUsagePct,
        ApplicationHistoryIdentity::verified_launcher("io.example.App").expect("identity"),
    );
    // Alternate percent spelling exercises the same private ingress as a published Unix colon token while remaining portable on Windows.
    let external =
        root.join("application-cpu-usage-pct__-__-__%6Cauncher%3Aio.example.%41pp.jsonl");
    let archive = "{\"r\":1,\"c\":100,\"m\":100,\"v\":12}\ncorrupt external line\n";
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(1000, u64::MAX), ALIVE)
            .expect("writer");
    record(&store, &key, sample(2, 200, Some(24.0)));
    store.flush(200).expect("current canonical segment");
    std::fs::write(&external, archive).expect("external read-only source");
    let query = HistoryQuery::new(&root);
    assert_eq!(
        query.known_series().expect("one identity"),
        std::slice::from_ref(&key)
    );
    let read = query
        .series(&key, HistoryWindow::OneHour, 200)
        .expect("query")
        .expect("segments");
    assert_eq!(read.series.samples.len(), 2);
    assert_eq!(read.series.samples[0].value, Some(12.0));
    assert_eq!(read.series.samples[1].value, Some(24.0));
    assert_eq!(read.corrupt_lines, 1);
    assert_eq!(
        std::fs::read_to_string(&external).expect("archive"),
        archive,
        "read ingress preserves external bytes and corruption evidence"
    );
    drop(store);
    cleanup(&root);
}

#[test]
fn retiring_an_external_segment_keeps_the_current_series_revision_guard() {
    let root = fixture_root("external-retirement");
    std::fs::create_dir_all(&root).expect("directory");
    let key = HistorySeriesKey::for_application(
        HistoryMetric::ApplicationCpuUsagePct,
        ApplicationHistoryIdentity::verified_launcher("io.example.App").expect("identity"),
    );
    let external =
        root.join("application-cpu-usage-pct__-__-__%6Cauncher%3Aio.example.%41pp.jsonl");
    std::fs::write(&external, "{\"r\":1,\"c\":10,\"m\":10,\"v\":12}\n").expect("old source");
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(100, u64::MAX), ALIVE)
            .expect("writer");
    record(&store, &key, sample(2, 200, Some(24.0)));
    store.flush(200).expect("retire old segment");
    assert!(!external.exists());
    assert_eq!(
        store.try_record_sample(key.clone(), sample(2, 210, Some(90.0))),
        RecordSampleOutcome::DuplicateRevision
    );
    let query = HistoryQuery::new(&root);
    assert_eq!(query.known_series().expect("retained current key"), [key]);
    drop(store);
    cleanup(&root);
}

#[test]
fn largest_admitted_filename_can_be_written_and_rewritten_by_retention() {
    let root = fixture_root("maximum-portable-component");
    let store =
        PersistentHistoryStore::open(&root, RetentionPolicy::for_tests(100, u64::MAX), ALIVE)
            .expect("writer");
    let prefix_bytes = HistorySeriesKey::for_device(HistoryMetric::GpuUsagePct, DeviceId::new("a"))
        .file_stem()
        .len()
        - 1;
    let key = HistorySeriesKey::for_device(
        HistoryMetric::GpuUsagePct,
        DeviceId::new("z".repeat(MAX_SERIES_KEY_BYTES - prefix_bytes)),
    );
    assert_eq!(key.file_stem().len(), MAX_SERIES_KEY_BYTES);
    for value in [sample(1, 10, Some(12.0)), sample(2, 200, Some(24.0))] {
        assert_eq!(
            store.try_record_sample(key.clone(), value),
            RecordSampleOutcome::Accepted
        );
    }
    assert_eq!(
        store
            .flush(200)
            .expect("write and TTL rewrite")
            .ttl_trimmed_files,
        1
    );
    let query = store.query();
    let read = query
        .series(&key, HistoryWindow::OneHour, 200)
        .expect("query")
        .expect("ordinary maximum-length file");
    assert_eq!(read.series.samples, [sample(2, 200, Some(24.0))]);
    let oversized = HistorySeriesKey::for_device(
        HistoryMetric::GpuUsagePct,
        DeviceId::new("z".repeat(MAX_SERIES_KEY_BYTES - prefix_bytes + 1)),
    );
    assert!(matches!(
        store.try_record_sample(oversized, sample(1, 200, Some(1.0))),
        RecordSampleOutcome::Rejected(RecordSampleRejection::SeriesKeyTooLong { .. })
    ));
    assert_eq!(query.known_series().expect("inventory"), [key]);
    drop(store);
    cleanup(&root);
}
