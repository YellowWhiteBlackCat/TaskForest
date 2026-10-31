//! Behavior tests for the timestamped trend read (CORE-48).
//!
//! The shared trend window must expose the sampling instant of every sample:
//! a hover pill reads the real moment a value was observed, never a sample
//! offset such as `-3`.

use crate::presentation::trend;
use taskforest_core::MetricSample;
use taskforest_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, CpuTelemetryObservation, ScalarObservation,
};
use taskforest_core::core::time::{LocalTimeRules, LocalTimeRulesObservation};
use taskmanager_telemetry_store::CorrelatedTelemetryStamp;
use taskmanager_telemetry_store::live_graph::LiveGraphHistory;

fn stamp(revision: u64, completed_at_ms: u64) -> CorrelatedTelemetryStamp {
    CorrelatedTelemetryStamp::from_accepted_event(revision, completed_at_ms)
        .expect("test revisions are non-zero")
}

fn observed_cpu(global_pct: f32, at_ms: u64) -> CpuTelemetryObservation {
    CpuTelemetryObservation::current(
        CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(global_pct, at_ms),
            ..Default::default()
        }),
        at_ms,
        Vec::new(),
    )
}

fn utc_rules() -> LocalTimeRulesObservation {
    LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0)
}

#[test]
fn the_timestamped_window_reports_each_sampling_instant() {
    let (history, ingestor) = LiveGraphHistory::shared(8);
    ingestor
        .ingest_correlated_cpu(
            stamp(1, 1_600_000_000_500),
            &observed_cpu(25.0, 1_600_000_000_000),
        )
        .expect("first cpu event appends");
    ingestor
        .ingest_correlated_cpu(
            stamp(2, 1_600_000_060_500),
            &observed_cpu(50.0, 1_600_000_060_000),
        )
        .expect("second cpu event appends");

    let samples = trend::timestamped_window(&history, trend::TrendSeries::CpuUsagePercent);
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].value(), Some(25.0));
    assert_eq!(samples[0].measured_at_ms(), Some(1_600_000_000_000));
    assert_eq!(samples[1].value(), Some(50.0));
    assert_eq!(samples[1].measured_at_ms(), Some(1_600_000_060_000));
}

#[test]
fn the_sampling_instant_is_shown_as_a_real_timestamp_not_an_offset() {
    let (history, ingestor) = LiveGraphHistory::shared(8);
    ingestor
        .ingest_correlated_cpu(
            stamp(1, 1_600_000_000_500),
            &observed_cpu(25.0, 1_600_000_000_000),
        )
        .expect("first cpu event appends");

    let samples = trend::timestamped_window(&history, trend::TrendSeries::CpuUsagePercent);
    let text = trend::sample_timestamp_text(&samples[0], &utc_rules());
    assert_eq!(text, "2020-09-13 12:26:40");
    assert!(
        !text.starts_with('-'),
        "the readout must not fall back to a sample offset"
    );
}

#[test]
fn a_sample_without_a_measurement_time_shows_no_instant() {
    let (history, ingestor) = LiveGraphHistory::shared(8);
    ingestor
        .ingest_correlated_cpu(
            stamp(1, 1_600_000_000_500),
            &observed_cpu(25.0, 1_600_000_000_000),
        )
        .expect("first cpu event appends");

    let samples = trend::timestamped_window(&history, trend::TrendSeries::CpuUsagePercent);
    let gap = MetricSample::gap(1_600_000_001_000);
    assert_eq!(trend::sample_timestamp_text(&gap, &utc_rules()), "—");
    assert!(!samples.is_empty());
}

#[test]
fn the_timestamped_window_and_the_value_window_read_the_same_samples() {
    let (history, ingestor) = LiveGraphHistory::shared(8);
    ingestor
        .ingest_correlated_cpu(
            stamp(1, 1_600_000_000_500),
            &observed_cpu(25.0, 1_600_000_000_000),
        )
        .expect("first cpu event appends");

    let samples = trend::timestamped_window(&history, trend::TrendSeries::CpuUsagePercent);
    let values = trend::window(&history, trend::TrendSeries::CpuUsagePercent);
    assert_eq!(samples.len(), values.len());
    for (sample, value) in samples.iter().zip(values.iter()) {
        match sample.value() {
            Some(expected) => assert_eq!(*value, expected),
            None => assert!(value.is_nan()),
        }
    }
}
