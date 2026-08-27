use super::*;

#[test]
fn metric_slugs_round_trip_and_stay_unique() {
    let mut slugs = Vec::new();
    for metric in HistoryMetric::ALL {
        let slug = metric.slug();
        assert_eq!(HistoryMetric::from_slug(slug), Some(metric));
        slugs.push(slug);
    }
    let unique: std::collections::HashSet<&str> = slugs.iter().copied().collect();
    assert_eq!(
        unique.len(),
        HistoryMetric::ALL.len(),
        "metric slugs must be unique: {slugs:?}"
    );
    assert_eq!(HistoryMetric::from_slug("not-a-metric"), None);
}

#[test]
fn series_key_stems_round_trip_across_scope_shapes() {
    let keys = [
        HistorySeriesKey::system(HistoryMetric::CpuUsagePct),
        HistorySeriesKey::for_core(HistoryMetric::CpuCoreUsagePct, 17),
        HistorySeriesKey::for_device(HistoryMetric::GpuUsagePct, DeviceId::new("card0")),
        HistorySeriesKey::for_device(
            HistoryMetric::NetworkRateBps,
            DeviceId::new(String::from("wlan0 ap/bridge %")),
        ),
        HistorySeriesKey::for_application(
            HistoryMetric::ApplicationCpuUsagePct,
            ApplicationHistoryIdentity::verified_launcher("io.github.Example App/%")
                .expect("fixture identity"),
        ),
        HistorySeriesKey::for_application(
            HistoryMetric::ApplicationMemoryBytes,
            ApplicationHistoryIdentity::unverified_process_name("standalone worker")
                .expect("fixture identity"),
        ),
    ];
    for key in keys {
        let stem = key.file_stem();
        assert!(
            !stem.contains('/'),
            "stems must be single path components: {stem}"
        );
        assert_eq!(HistorySeriesKey::from_file_stem(&stem), Some(key));
    }
}

#[test]
fn malformed_stems_are_rejected_without_panicking() {
    for stem in [
        "cpu-usage-pct",
        "cpu-usage-pct__-__-__-",
        "not-a-metric__-__-",
        "gpu-usage-pct__%2G__-",
        "cpu-core-usage-pct__-__three",
        "cpu-core-usage-pct__-__-1",
        "application-cpu-usage-pct__-__-__unknown:value",
        "application-cpu-usage-pct__card0__-__launcher:app",
        "application-cpu-usage-pct__-__-",
        "cpu-usage-pct__-__-__launcher:app",
    ] {
        assert_eq!(
            HistorySeriesKey::from_file_stem(stem),
            None,
            "stem must be rejected: {stem}"
        );
    }
    // The empty device id (bare `__` scope) is rejected; the unscoped
    // '-' stays valid. An encoded NUL round-trips like any other byte —
    // it can only originate from `file_stem`, never a hostile name.
    assert_eq!(
        HistorySeriesKey::from_file_stem("gpu-usage-pct____-"),
        None,
        "a decoded empty device id must not become a phantom series"
    );
    assert_eq!(
        HistorySeriesKey::from_file_stem("cpu-usage-pct__-__-"),
        Some(HistorySeriesKey::system(HistoryMetric::CpuUsagePct))
    );
}

#[test]
fn application_metrics_require_an_application_scope() {
    let invalid = HistorySeriesKey::system(HistoryMetric::ApplicationCpuUsagePct);
    assert!(!invalid.is_valid());
    let valid = HistorySeriesKey::for_application(
        HistoryMetric::ApplicationCpuUsagePct,
        ApplicationHistoryIdentity::verified_launcher("io.example.App").expect("fixture identity"),
    );
    assert!(valid.is_valid());
}

#[test]
fn windows_have_distinct_durations() {
    let durations: Vec<u64> = HistoryWindow::ALL
        .iter()
        .map(|window| window.duration_ms())
        .collect();
    let unique: std::collections::HashSet<u64> = durations.iter().copied().collect();
    assert_eq!(unique.len(), HistoryWindow::ALL.len());
    assert_eq!(HistoryWindow::SevenDays.duration_ms(), 7 * 24 * 3600 * 1000);
}

#[test]
fn series_peak_ignores_gaps_and_counts_them() {
    let key = HistorySeriesKey::system(HistoryMetric::MemoryUsedPct);
    let series = HistoricalSeries::new(
        key.clone(),
        vec![
            HistoricalSample {
                revision: 1,
                completed_at_ms: 1_000,
                measured_at_ms: Some(1_000),
                value: Some(41.0),
            },
            HistoricalSample {
                revision: 2,
                completed_at_ms: 2_000,
                measured_at_ms: None,
                value: None,
            },
            HistoricalSample {
                revision: 3,
                completed_at_ms: 3_000,
                measured_at_ms: Some(3_000),
                value: Some(87.5),
            },
        ],
    );
    assert_eq!(series.gap_count(), 1);
    let peak = series.peak().expect("a measured peak exists");
    assert_eq!(peak.value, Some(87.5));
    assert_eq!(peak.measured_at_ms, Some(3_000));
    assert_eq!(series.clock_jumps, 0);
}

#[test]
fn backward_clock_steps_are_counted_and_forward_gaps_are_not() {
    let sample = |revision: u64, completed_at_ms: u64| HistoricalSample {
        revision,
        completed_at_ms,
        measured_at_ms: Some(completed_at_ms),
        value: Some(1.0),
    };
    // Suspend-style forward jump: a time gap, not a clock jump.
    assert_eq!(
        count_clock_jumps(&[sample(1, 1_000), sample(2, 9_000_000)]),
        0
    );
    // NTP-style correction backwards: one jump, and the series keeps both
    // samples as recorded.
    assert_eq!(
        count_clock_jumps(&[
            sample(1, 5_000_000),
            sample(2, 4_900_000),
            sample(3, 4_950_000)
        ]),
        1
    );
    // Two separate backwards steps both count.
    assert_eq!(
        count_clock_jumps(&[sample(1, 5_000), sample(2, 4_000), sample(3, 3_000)]),
        2
    );
    assert_eq!(count_clock_jumps(&[]), 0);
}
