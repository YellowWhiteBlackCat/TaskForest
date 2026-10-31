//! Timestamped chart series reads over the live graph read model.
//!
//! The value-only window cannot say when a value was sampled. These tests
//! pin the timestamped read: every accepted sample carries the instant its
//! observation was measured at, the two reads stay index-aligned, and a
//! missing measurement stays a gap instead of becoming a synthetic instant.

use crate::HistoryRetention;
use taskforest_core::{
    CpuMetrics, CpuScalarObservations, CpuTelemetryObservation, FailureKind, MetricSample,
    ScalarObservation,
};

use super::{ChartSeriesError, ChartSeriesQuery, DeviceDomain, LiveGraphHistory, MetricSeries};
use crate::{CorrelatedTelemetryStamp, SystemHistoryDomain, TelemetryStore};

fn stamp_at(revision: u64, completed_at_ms: u64) -> CorrelatedTelemetryStamp {
    CorrelatedTelemetryStamp::from_accepted_event(revision, completed_at_ms)
        .expect("test revisions are non-zero")
}

fn cpu(usage: f32, observed_at_ms: u64) -> CpuTelemetryObservation {
    CpuTelemetryObservation::current(
        CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(usage, observed_at_ms),
            ..Default::default()
        }),
        observed_at_ms,
        Vec::new(),
    )
}

fn cpu_samples(reader: &LiveGraphHistory) -> Vec<MetricSample> {
    reader
        .resolve_series_samples(ChartSeriesQuery::host(MetricSeries::CpuUsagePercent))
        .expect("the host cpu series resolves in its host domain")
}

#[test]
fn every_sample_carries_the_instant_its_observation_was_measured_at() {
    let (store, ingestor) =
        TelemetryStore::shared_with_correlated_ingestion(HistoryRetention::uniform(8));
    ingestor
        .ingest_correlated_cpu(stamp_at(1, 1_000), &cpu(25.0, 900))
        .expect("first cpu event appends");
    ingestor
        .ingest_correlated_cpu(stamp_at(2, 2_000), &cpu(50.0, 1_900))
        .expect("second cpu event appends");

    let reader = LiveGraphHistory::from_store(store, 8);
    let samples = cpu_samples(&reader);
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].value(), Some(25.0));
    assert_eq!(
        samples[0].measured_at_ms(),
        Some(900),
        "the sampling instant is the observation's own measurement time"
    );
    assert_eq!(samples[0].completed_at_ms(), 1_000);
    assert_eq!(samples[1].value(), Some(50.0));
    assert_eq!(samples[1].measured_at_ms(), Some(1_900));
    assert_eq!(samples[1].completed_at_ms(), 2_000);
}

#[test]
fn the_value_window_and_the_timestamped_window_stay_index_aligned() {
    let (store, ingestor) =
        TelemetryStore::shared_with_correlated_ingestion(HistoryRetention::uniform(8));
    ingestor
        .ingest_correlated_cpu(stamp_at(1, 1_000), &cpu(25.0, 900))
        .expect("first cpu event appends");
    ingestor
        .ingest_correlated_unavailable(
            stamp_at(2, 2_000),
            SystemHistoryDomain::Cpu,
            FailureKind::TemporarilyUnavailable,
        )
        .expect("the failed domain event appends an explicit gap");
    ingestor
        .ingest_correlated_cpu(stamp_at(3, 3_000), &cpu(50.0, 2_900))
        .expect("third cpu event appends");

    let reader = LiveGraphHistory::from_store(store, 8);
    let values = reader
        .resolve_series(ChartSeriesQuery::host(MetricSeries::CpuUsagePercent))
        .expect("the host cpu series resolves in its host domain");
    let samples = cpu_samples(&reader);
    assert_eq!(values.len(), samples.len());
    for (value, sample) in values.iter().zip(samples.iter()) {
        match sample.value() {
            Some(expected) => assert_eq!(*value, expected),
            None => assert!(value.is_nan(), "a gap stays a NaN gap in the value window"),
        }
    }
}

#[test]
fn an_observation_without_a_measurement_time_stays_a_gap() {
    let (store, ingestor) =
        TelemetryStore::shared_with_correlated_ingestion(HistoryRetention::uniform(8));
    ingestor
        .ingest_correlated_cpu(stamp_at(1, 1_000), &cpu(25.0, 900))
        .expect("first cpu event appends");
    ingestor
        .ingest_correlated_unavailable(
            stamp_at(2, 2_000),
            SystemHistoryDomain::Cpu,
            FailureKind::TemporarilyUnavailable,
        )
        .expect("the failed domain event appends an explicit gap");

    let reader = LiveGraphHistory::from_store(store, 8);
    let samples = cpu_samples(&reader);
    assert_eq!(samples.len(), 2);
    assert!(samples[1].is_gap());
    assert_eq!(samples[1].value(), None);
    assert_eq!(
        samples[1].measured_at_ms(),
        None,
        "a sample without a measurement time has no instant to show"
    );
    assert_eq!(
        samples[1].completed_at_ms(),
        2_000,
        "the accepted event's completion time still orders the gap"
    );
}

#[test]
fn a_wrong_domain_query_is_rejected_by_the_timestamped_read_too() {
    let (store, _ingestor) =
        TelemetryStore::shared_with_correlated_ingestion(HistoryRetention::uniform(8));
    let reader = LiveGraphHistory::from_store(store, 8);
    assert_eq!(
        reader.resolve_series_samples(ChartSeriesQuery::host(MetricSeries::DiskActiveTimePct)),
        Err(ChartSeriesError::MissingDeviceIdentity {
            series: MetricSeries::DiskActiveTimePct,
            domain: DeviceDomain::Storage,
        })
    );
}
