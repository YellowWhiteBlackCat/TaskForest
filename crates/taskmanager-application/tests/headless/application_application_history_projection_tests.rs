use std::sync::Arc;

use super::*;
use crate::{
    ApplicationHistoryCapability, ApplicationHistoryIdentity, ApplicationHistoryStatus,
    ApplicationHistoryUnavailableReason, HistoryMetric, HistoryReplayError, HistoryReplayErrorKind,
    HistoryReplayRow, HistorySeriesKey, HistoryWindow,
};

fn replay_row(
    identity: ApplicationHistoryIdentity,
    metric: HistoryMetric,
    samples: &[f32],
    peak: Option<f64>,
) -> HistoryReplayRow {
    HistoryReplayRow {
        key: HistorySeriesKey::for_application(metric, identity),
        samples: Arc::from(samples),
        sample_times_ms: Arc::from(
            (0..samples.len())
                .map(|index| 1_000 + index as u64 * 1_000)
                .collect::<Vec<_>>(),
        ),
        peak_value: peak,
        peak_measured_at_ms: Some(1_000),
        observed: samples.iter().filter(|sample| sample.is_finite()).count(),
        gaps: samples.iter().filter(|sample| !sample.is_finite()).count(),
        clock_jumps: 0,
    }
}

#[test]
fn replay_metrics_join_by_typed_identity_and_rank_by_window_cpu_peak() {
    let alpha = ApplicationHistoryIdentity::verified_launcher("io.example.Alpha")
        .expect("verified identity");
    let worker =
        ApplicationHistoryIdentity::unverified_process_name("worker").expect("fallback identity");
    let rows = vec![
        replay_row(
            alpha.clone(),
            HistoryMetric::ApplicationMemoryBytes,
            &[100.0, 250.0],
            Some(250.0),
        ),
        replay_row(
            worker.clone(),
            HistoryMetric::ApplicationCpuUsagePct,
            &[7.0, 30.0],
            Some(30.0),
        ),
        replay_row(
            alpha.clone(),
            HistoryMetric::ApplicationCpuUsagePct,
            &[2.0, 9.0],
            Some(9.0),
        ),
        replay_row(
            alpha.clone(),
            HistoryMetric::ApplicationProcessCount,
            &[1.0, 3.0],
            Some(3.0),
        ),
        HistoryReplayRow {
            key: HistorySeriesKey::system(HistoryMetric::CpuUsagePct),
            samples: Arc::from([99.0]),
            sample_times_ms: Arc::from([1_000]),
            peak_value: Some(99.0),
            peak_measured_at_ms: Some(1_000),
            observed: 1,
            gaps: 0,
            clock_jumps: 0,
        },
    ];

    let projected = project_application_history_rows(&rows);
    assert_eq!(projected.len(), 2);
    assert_eq!(projected[0].identity, worker);
    assert_eq!(projected[0].peak_cpu_usage_pct(), Some(30.0));
    assert!(projected[0].memory.is_none());
    assert_eq!(projected[1].identity, alpha);
    assert_eq!(projected[1].peak_memory_bytes(), Some(250.0));
    assert_eq!(projected[1].peak_process_count(), Some(3.0));
    assert_eq!(
        projected[1]
            .cpu_usage
            .as_ref()
            .map(|series| series.samples.as_ref()),
        Some([2.0, 9.0].as_slice())
    );
}

#[test]
fn collector_downtime_becomes_an_explicit_sparkline_gap() {
    let series = crate::ApplicationHistoryMetricSeries {
        samples: Arc::from([1.0, 2.0, 4.0]),
        sample_times_ms: Arc::from([1_000, 2_000, 20_000]),
        peak_value: Some(4.0),
        peak_measured_at_ms: Some(20_000),
        observed: 3,
        gaps: 0,
        clock_jumps: 0,
    };
    let projected = series.gap_aware_samples();
    assert_eq!(&projected[..2], &[1.0, 2.0]);
    assert!(projected[2].is_nan());
    assert_eq!(projected[3], 4.0);
}

fn timed_series(samples: &[f32], times: &[u64]) -> crate::ApplicationHistoryMetricSeries {
    crate::ApplicationHistoryMetricSeries {
        samples: Arc::from(samples),
        sample_times_ms: Arc::from(times),
        peak_value: None,
        peak_measured_at_ms: None,
        observed: samples.iter().filter(|sample| sample.is_finite()).count(),
        gaps: 0,
        clock_jumps: 0,
    }
}

#[test]
fn two_samples_still_break_when_the_interval_exceeds_the_absolute_cadence_bound() {
    let projected = timed_series(&[1.0, 2.0], &[1_000, 181_001]).gap_aware_samples();
    assert_eq!(projected[0], 1.0);
    assert!(projected[1].is_nan());
    assert_eq!(projected[2], 2.0);
}

#[test]
fn clock_reversal_breaks_while_a_normal_sixty_second_interval_does_not() {
    let reversed = timed_series(&[1.0, 2.0], &[2_000, 1_000]).gap_aware_samples();
    assert!(reversed[1].is_nan());
    let normal = timed_series(&[1.0, 2.0], &[1_000, 61_000]).gap_aware_samples();
    assert_eq!(normal.as_ref(), &[1.0, 2.0]);
}

#[test]
fn reader_capability_and_empty_replay_have_explicit_page_states() {
    let failure = HistoryReplayError::new(HistoryReplayErrorKind::Read, "reader unavailable");
    for (capability, failure, expected) in [
        (
            ApplicationHistoryCapability::Disabled,
            None,
            ApplicationHistoryStatus::Disabled,
        ),
        (
            ApplicationHistoryCapability::Unavailable(
                ApplicationHistoryUnavailableReason::ConnectorStopped,
            ),
            None,
            ApplicationHistoryStatus::Unavailable,
        ),
        (
            ApplicationHistoryCapability::Connecting,
            None,
            ApplicationHistoryStatus::Connecting,
        ),
        (
            ApplicationHistoryCapability::Available,
            None,
            ApplicationHistoryStatus::Collecting,
        ),
        (
            ApplicationHistoryCapability::Available,
            Some(failure),
            ApplicationHistoryStatus::Unavailable,
        ),
    ] {
        let projection =
            crate::ApplicationHistoryProjection::from_replay(ApplicationHistoryReplaySnapshot {
                capability,
                selected_window: HistoryWindow::OneHour,
                rows_window: None,
                rows: Arc::from([]),
                source_request: None,
                refreshing: false,
                failure,
                loaded_at_ms: None,
            });
        assert_eq!(projection.status, expected);
        assert!(projection.rows.is_empty());
    }
}
