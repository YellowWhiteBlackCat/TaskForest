use super::project_timeline;
use std::sync::Arc;
use taskmanager_application::MIN_TELEMETRY_INTERVAL;
use taskmanager_application::system_timeline::{
    MAX_GRAPH_POINTS, SystemHistoryWindow, TimelineMetric, TimelineSelection, TimelineState,
    TimelineStatistic,
};
use taskmanager_core::core::{
    CpuMetrics, CpuScalarObservations, CpuTelemetryObservation, FailureKind, ScalarObservation,
};
use taskmanager_telemetry_store::HistoryRetention;
use taskmanager_telemetry_store::{
    CorrelatedSystemTelemetryIngestor, CorrelatedTelemetryStamp, TelemetryStore,
};

struct TimelineHarness {
    store: Arc<TelemetryStore>,
    ingestor: CorrelatedSystemTelemetryIngestor,
    revision: u64,
}

impl TimelineHarness {
    fn new(capacity: usize) -> Self {
        let (store, ingestor) =
            TelemetryStore::shared_with_correlated_ingestion(HistoryRetention::uniform(capacity));
        Self {
            store,
            ingestor,
            revision: 0,
        }
    }

    fn stamp(&mut self, completed_at_ms: u64) -> CorrelatedTelemetryStamp {
        self.revision = self.revision.saturating_add(1);
        CorrelatedTelemetryStamp::from_accepted_event(self.revision, completed_at_ms)
            .expect("the bounded fixture never exhausts non-zero revisions")
    }

    fn cpu(&mut self, completed_at_ms: u64, value: f32) {
        let stamp = self.stamp(completed_at_ms);
        let observation = CpuTelemetryObservation::current(
            CpuMetrics::from_observations(CpuScalarObservations {
                global_usage_pct: ScalarObservation::available(value, completed_at_ms),
                ..Default::default()
            }),
            completed_at_ms,
            Vec::new(),
        );
        self.ingestor
            .ingest_correlated_cpu(stamp, &observation)
            .expect("increasing correlated CPU fixture");
    }

    fn cpu_gap(&mut self, completed_at_ms: u64) {
        let stamp = self.stamp(completed_at_ms);
        self.ingestor
            .ingest_correlated_cpu(
                stamp,
                &CpuTelemetryObservation::unavailable(FailureKind::PermissionDenied, Vec::new()),
            )
            .expect("increasing correlated CPU gap fixture");
    }
}

/// The GPUI memo is a pure projection of the telemetry-store watermark: a
/// rejected duplicate cannot mutate it, while an accepted event must.
#[test]
fn series_memo_tracks_authoritative_history_not_frontend_snapshots() {
    let mut harness = TimelineHarness::new(16);
    let timeline = TimelineState::default();
    harness.cpu(10_000, 10.0);
    let first = project_timeline(
        &timeline,
        &harness.store.system_history,
        SystemHistoryWindow::OneMinute,
    );
    let second = project_timeline(
        &timeline,
        &harness.store.system_history,
        SystemHistoryWindow::OneMinute,
    );
    assert!(Arc::ptr_eq(&first.cpu_percent, &second.cpu_percent));

    let duplicate = CorrelatedTelemetryStamp::from_accepted_event(1, 11_000)
        .expect("fixture revision is non-zero");
    let rejected = harness.ingestor.ingest_correlated_cpu(
        duplicate,
        &CpuTelemetryObservation::current(
            CpuMetrics::from_observations(CpuScalarObservations {
                global_usage_pct: ScalarObservation::available(99.0, 11_000),
                ..Default::default()
            }),
            11_000,
            Vec::new(),
        ),
    );
    assert!(rejected.is_err(), "duplicate correlation must be rejected");
    let after_rejection = project_timeline(
        &timeline,
        &harness.store.system_history,
        SystemHistoryWindow::OneMinute,
    );
    assert!(Arc::ptr_eq(
        &second.cpu_percent,
        &after_rejection.cpu_percent
    ));

    harness.cpu(20_000, 42.0);
    let after_accept = project_timeline(
        &timeline,
        &harness.store.system_history,
        SystemHistoryWindow::OneMinute,
    );
    assert!(!Arc::ptr_eq(
        &after_rejection.cpu_percent,
        &after_accept.cpu_percent
    ));
    assert_eq!(
        after_accept
            .cpu_percent
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .collect::<Vec<_>>(),
        [10.0, 42.0]
    );
    assert!(
        after_accept.cpu_percent[0].is_nan(),
        "unobserved prefix occupies its real time range"
    );

    let other_window = project_timeline(
        &timeline,
        &harness.store.system_history,
        SystemHistoryWindow::SixtyMinutes,
    );
    assert!(!Arc::ptr_eq(
        &after_accept.cpu_percent,
        &other_window.cpu_percent
    ));
}

#[test]
fn windows_use_correlated_completion_time_and_bound_graph_points() {
    let mut harness = TimelineHarness::new(600);
    let timeline = TimelineState::default();
    for index in 0..600_u64 {
        let value = u16::try_from(index % 100).map_or(0.0, f32::from);
        harness.cpu(index.saturating_mul(10_000), value);
    }
    let one = project_timeline(
        &timeline,
        &harness.store.system_history,
        SystemHistoryWindow::OneMinute,
    );
    let sixty = project_timeline(
        &timeline,
        &harness.store.system_history,
        SystemHistoryWindow::SixtyMinutes,
    );
    assert_eq!(one.covered_ms, 60_000);
    assert_eq!(one.cpu_percent.len(), 7);
    assert_eq!(sixty.covered_ms, 3_600_000);
    assert!(sixty.cpu_percent.len() <= MAX_GRAPH_POINTS);
}

#[test]
fn latest_peak_and_each_window_are_derived_from_the_same_cpu_ring() {
    let mut harness = TimelineHarness::new(16);
    let timeline = TimelineState::default();
    for (minute, cpu) in [(0, 99.0), (45, 70.0), (55, 60.0), (59, 50.0), (60, 10.0)] {
        harness.cpu(minute * 60_000, cpu);
    }
    let latest = project_timeline(
        &timeline,
        &harness.store.system_history,
        SystemHistoryWindow::SixtyMinutes,
    )
    .readout(TimelineSelection::new(
        TimelineMetric::Cpu,
        TimelineStatistic::Latest,
    ))
    .expect("finite latest sample");
    assert_eq!((latest.sample_index, latest.value), (4, 10.0));

    let selection = TimelineSelection::new(TimelineMetric::Cpu, TimelineStatistic::Peak);
    for (window, expected) in [
        (SystemHistoryWindow::OneMinute, 50.0),
        (SystemHistoryWindow::FiveMinutes, 60.0),
        (SystemHistoryWindow::FifteenMinutes, 70.0),
        (SystemHistoryWindow::SixtyMinutes, 99.0),
    ] {
        assert_eq!(
            project_timeline(&timeline, &harness.store.system_history, window)
                .readout(selection)
                .expect("window contains a finite sample")
                .value,
            expected
        );
    }
}

#[test]
fn typed_unavailable_samples_remain_gaps_and_readouts_skip_them() {
    let mut harness = TimelineHarness::new(16);
    let timeline = TimelineState::default();
    harness.cpu(1_000, 12.5);
    harness.cpu_gap(2_000);
    harness.cpu(3_000, 37.0);

    let series = project_timeline(
        &timeline,
        &harness.store.system_history,
        SystemHistoryWindow::OneMinute,
    );
    let tail = &series.cpu_percent[series.cpu_percent.len() - 3..];
    assert_eq!(tail[0], 12.5);
    assert!(tail[1].is_nan());
    assert_eq!(tail[2], 37.0);
    assert_eq!(
        series
            .readout(TimelineSelection::new(
                TimelineMetric::Cpu,
                TimelineStatistic::Latest,
            ))
            .expect("finite tail exists")
            .value,
        37.0
    );
}

#[test]
fn latest_readout_uses_raw_tail_not_peak_downsample_bucket() {
    let mut harness = TimelineHarness::new(600);
    let timeline = TimelineState::default();
    for index in 0..597_u64 {
        harness.cpu(index.saturating_mul(1_000), 1.0);
    }
    for (index, value) in [(597, 10.0), (598, 90.0), (599, 20.0)] {
        harness.cpu(index * 1_000, value);
    }
    let series = project_timeline(
        &timeline,
        &harness.store.system_history,
        SystemHistoryWindow::SixtyMinutes,
    );
    assert_eq!(
        series
            .readout(TimelineSelection::new(
                TimelineMetric::Cpu,
                TimelineStatistic::Latest,
            ))
            .expect("raw tail is finite")
            .value,
        20.0
    );
    assert_eq!(
        series
            .readout(TimelineSelection::new(
                TimelineMetric::Cpu,
                TimelineStatistic::Peak,
            ))
            .expect("raw peak is finite")
            .value,
        90.0
    );
    assert_eq!(
        series.cpu_percent.last().copied(),
        Some(20.0),
        "the last real timestamp remains distinct from preceding peak buckets"
    );
}

#[test]
fn wall_clock_rollback_starts_a_new_tail_anchored_window() {
    let mut harness = TimelineHarness::new(16);
    let timeline = TimelineState::default();
    harness.cpu(100_000, 10.0);
    harness.cpu(1_000, 20.0);

    let series = project_timeline(
        &timeline,
        &harness.store.system_history,
        SystemHistoryWindow::OneMinute,
    );
    assert_eq!(series.cpu_percent.last().copied(), Some(20.0));
    assert_eq!(
        series
            .cpu_percent
            .iter()
            .filter(|value| value.is_finite())
            .count(),
        1
    );
    assert_eq!(series.covered_ms, 0);
    assert_eq!(
        series
            .readout(TimelineSelection::new(
                TimelineMetric::Cpu,
                TimelineStatistic::Latest,
            ))
            .expect("post-rollback sample")
            .value,
        20.0
    );
}

#[test]
fn downsample_keeps_all_gap_chunks_as_nan() {
    let mut harness = TimelineHarness::new(500);
    let timeline = TimelineState::default();
    for index in 0..490_u64 {
        harness.cpu_gap(index.saturating_mul(1_000));
    }
    for index in 490..500_u64 {
        harness.cpu(index.saturating_mul(1_000), 7.5);
    }
    let series = project_timeline(
        &timeline,
        &harness.store.system_history,
        SystemHistoryWindow::SixtyMinutes,
    );
    assert!(series.cpu_percent.len() <= MAX_GRAPH_POINTS && series.cpu_percent.len() > 1);
    assert!(series.cpu_percent[0].is_nan());
    assert!(series.cpu_percent[series.cpu_percent.len() - 1].is_finite());
    assert_eq!(
        series
            .readout(TimelineSelection::new(
                TimelineMetric::Cpu,
                TimelineStatistic::Latest,
            ))
            .expect("finite raw tail after mixed gap buckets")
            .value,
        7.5
    );
}

#[test]
fn an_unavailable_tail_has_no_current_readout_but_preserves_the_window_peak() {
    let mut harness = TimelineHarness::new(16);
    harness.cpu(1_000, 12.5);
    harness.cpu_gap(2_000);
    let series = project_timeline(
        &TimelineState::default(),
        &harness.store.system_history,
        SystemHistoryWindow::OneMinute,
    );
    assert!(
        series
            .readout(TimelineSelection::new(
                TimelineMetric::Cpu,
                TimelineStatistic::Latest
            ))
            .is_none()
    );
    assert_eq!(
        series
            .readout(TimelineSelection::new(
                TimelineMetric::Cpu,
                TimelineStatistic::Peak
            ))
            .expect("observed peak")
            .value,
        12.5
    );
    assert!(series.cpu_percent.last().expect("gap cell").is_nan());
}

#[test]
fn irregular_collection_keeps_downtime_in_its_real_horizontal_slots() {
    let mut harness = TimelineHarness::new(16);
    for (time, value) in [
        (0, 0.0),
        (1_000, 1.0),
        (2_000, 2.0),
        (10_000, 10.0),
        (11_000, 11.0),
    ] {
        harness.cpu(time, value);
    }
    let series = project_timeline(
        &TimelineState::default(),
        &harness.store.system_history,
        SystemHistoryWindow::OneMinute,
    );
    assert_eq!(series.cpu_percent.len(), 61);
    assert_eq!(series.cpu_percent[49], 0.0, "an observed zero remains data");
    assert_eq!(series.cpu_percent[51], 2.0);
    assert!(
        series.cpu_percent[52..59]
            .iter()
            .all(|value| value.is_nan())
    );
    assert_eq!(
        series.cpu_percent[59], 10.0,
        "the resumed observation remains reachable"
    );
    assert_eq!(series.cpu_percent[60], 11.0);
}

#[test]
fn production_retention_covers_a_full_hour_at_the_minimum_refresh_interval() {
    use taskmanager_telemetry_store::{DETAIL_HISTORY_CAPACITY, HOST_AGGREGATE_HISTORY_CAPACITY};
    let interval = u64::try_from(MIN_TELEMETRY_INTERVAL.as_millis()).expect("bounded cadence");
    assert_eq!(
        u64::try_from(HOST_AGGREGATE_HISTORY_CAPACITY).expect("bounded capacity"),
        3_600_000 / interval + 1
    );
    let (store, ingestor) =
        TelemetryStore::shared_with_correlated_ingestion(HistoryRetention::PRODUCT);
    let mut harness = TimelineHarness {
        store,
        ingestor,
        revision: 0,
    };
    for index in 0..=HOST_AGGREGATE_HISTORY_CAPACITY {
        let value = match index {
            0 => 200.0,
            1 => 99.0,
            _ => 1.0,
        };
        harness.cpu(
            u64::try_from(index).expect("bounded sample") * interval,
            value,
        );
    }
    let history = &harness.store.system_history;
    assert_eq!(
        history.cpu_usage().samples().len(),
        HOST_AGGREGATE_HISTORY_CAPACITY
    );
    assert_eq!(
        history.cpu_frequency_mhz().capacity(),
        DETAIL_HISTORY_CAPACITY
    );
    assert_eq!(
        history.memory_usage().capacity(),
        HOST_AGGREGATE_HISTORY_CAPACITY
    );
    assert_eq!(
        history.storage_rate_total().capacity(),
        HOST_AGGREGATE_HISTORY_CAPACITY
    );
    assert_eq!(
        history.network_rate_total().capacity(),
        HOST_AGGREGATE_HISTORY_CAPACITY
    );
    let series = project_timeline(
        &TimelineState::default(),
        history,
        SystemHistoryWindow::SixtyMinutes,
    );
    assert_eq!(series.coverage_ms(TimelineMetric::Cpu), 3_600_000);
    assert_eq!(
        series
            .readout(TimelineSelection::new(
                TimelineMetric::Cpu,
                TimelineStatistic::Peak
            ))
            .expect("retained hour peak")
            .value,
        99.0
    );
    assert_eq!(
        series
            .readout(TimelineSelection::new(
                TimelineMetric::Cpu,
                TimelineStatistic::Latest
            ))
            .expect("current tail")
            .value,
        1.0
    );
    assert!(series.cpu_percent.iter().all(|value| value.is_finite()));
    assert_eq!(series.cpu_percent.len(), MAX_GRAPH_POINTS);
}
