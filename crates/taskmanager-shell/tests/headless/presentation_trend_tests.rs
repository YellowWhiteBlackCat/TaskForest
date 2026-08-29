//! Behavior tests for the shell trend projections (ADR-027 behavior seam).

use crate::presentation::trend;
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, CpuTelemetryObservation, MemoryMetrics,
    MemoryScalarObservations, MemoryTelemetryObservation, ScalarObservation,
    ScalarObservationGroup,
};
use taskmanager_telemetry_store::CorrelatedTelemetryStamp;
use taskmanager_telemetry_store::live_graph::LiveGraphHistory;

fn stamp(revision: u64) -> CorrelatedTelemetryStamp {
    CorrelatedTelemetryStamp::from_accepted_event(revision, revision * 10)
        .expect("test revisions are non-zero")
}

fn observed_cpu(global_pct: f32, cores: &[f32], at_ms: u64) -> CpuTelemetryObservation {
    CpuTelemetryObservation::current(
        CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(global_pct, at_ms),
            core_usage_group: ScalarObservationGroup::available(cores.to_vec(), at_ms),
            ..Default::default()
        }),
        at_ms,
        Vec::new(),
    )
}

fn observed_memory(used_pct: f32, at_ms: u64) -> MemoryTelemetryObservation {
    // 100 GiB total keeps the used/total ratio exactly the requested percent.
    let total = 100 * 1024 * 1024 * 1024;
    let used = (f64::from(used_pct) / 100.0 * total as f64).round() as u64;
    MemoryTelemetryObservation::current(
        MemoryMetrics::from_observations(
            MemoryScalarObservations {
                total_bytes: ScalarObservation::available(total, at_ms),
                used_bytes: ScalarObservation::available(used, at_ms),
                ..Default::default()
            },
            Default::default(),
        ),
        at_ms,
        Vec::new(),
    )
}

#[test]
fn trend_windows_return_the_ingested_shared_history_samples() {
    let (history, ingestor) = LiveGraphHistory::shared(8);
    ingestor
        .ingest_correlated_cpu(stamp(1), &observed_cpu(25.0, &[10.0, 20.0], 10))
        .expect("first cpu event appends");
    ingestor
        .ingest_correlated_cpu(stamp(2), &observed_cpu(50.0, &[30.0, 40.0], 20))
        .expect("second cpu event appends");
    ingestor
        .ingest_correlated_memory(stamp(3), &observed_memory(37.5, 30))
        .expect("memory event appends");

    assert_eq!(trend::cpu_usage_percent(&history), vec![25.0, 50.0]);
    assert_eq!(trend::memory_usage_percent(&history), vec![37.5]);
    assert_eq!(
        trend::per_core_usage_percent(&history),
        vec![vec![10.0, 30.0], vec![20.0, 40.0]],
    );
}

#[test]
fn untouched_history_yields_empty_trend_windows() {
    let (history, _ingestor) = LiveGraphHistory::shared(8);
    // No fabricated samples: absence stays absence, never a zero-filled window.
    assert!(trend::cpu_usage_percent(&history).is_empty());
    assert!(trend::memory_usage_percent(&history).is_empty());
    assert!(trend::per_core_usage_percent(&history).is_empty());
}

#[test]
fn trend_series_slots_are_exhaustive_and_distinct() {
    // The selector vocabulary must cover the storage slots exactly once —
    // a new storage series cannot silently miss a renderer selector.
    let slots: Vec<usize> = trend::TrendSeries::ALL.iter().map(|s| s.slot()).collect();
    let distinct: std::collections::HashSet<usize> = slots.iter().copied().collect();
    assert_eq!(slots.len(), distinct.len(), "slots must be distinct");
    assert_eq!(
        slots.iter().max().copied(),
        Some(trend::TrendSeries::ALL.len() - 1),
        "slots must be contiguous"
    );
}

#[test]
fn window_read_matches_the_named_cpu_projection() {
    let (history, ingestor) = LiveGraphHistory::shared(8);
    ingestor
        .ingest_correlated_cpu(stamp(1), &observed_cpu(25.0, &[10.0, 20.0], 10))
        .expect("first cpu event appends");
    assert_eq!(
        trend::window(&history, trend::TrendSeries::CpuUsagePercent),
        trend::cpu_usage_percent(&history),
    );
}
