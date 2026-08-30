use super::*;
use std::sync::Mutex;
use taskmanager_core::{
    FailureKind, HistoricalSample, HistoryMetric, HistorySeriesKey, ProcessApplicationIdentity,
    ProcessMetadataObservation, ProcessScalarObservations, ScalarObservation,
};

#[derive(Default)]
struct RecordingSink(Mutex<Vec<(HistorySeriesKey, HistoricalSample)>>);

impl HistoryRecordSink for RecordingSink {
    fn record_sample(&self, key: HistorySeriesKey, sample: HistoricalSample) {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push((key, sample));
    }
}

fn process(pid: u32, name: &str, launcher: Option<&str>) -> ProcessItem {
    process_with_metrics(
        pid,
        name,
        launcher,
        ScalarObservation::available(7.5, 42),
        ScalarObservation::available(512, 42),
    )
}

fn process_with_metrics(
    pid: u32,
    name: &str,
    launcher: Option<&str>,
    cpu_percentage: ScalarObservation<f32>,
    memory_bytes: ScalarObservation<u64>,
) -> ProcessItem {
    let mut process = ProcessItem::new(pid, name);
    process.apply_scalar_observations(ProcessScalarObservations {
        cpu_percentage,
        memory_bytes,
        ..ProcessScalarObservations::default()
    });
    if let Some(launcher) = launcher {
        process.apply_application_identity(ProcessMetadataObservation::available(
            ProcessApplicationIdentity::new(launcher, name, None).expect("valid identity"),
            42,
        ));
    }
    process
}

fn records_for_metric(
    sink: &RecordingSink,
    metric: HistoryMetric,
    identity: &str,
) -> Vec<HistoricalSample> {
    sink.0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .filter(|(key, _)| {
            key.metric() == metric
                && key
                    .application()
                    .is_some_and(|value| value.value() == identity)
        })
        .map(|(_, sample)| *sample)
        .collect()
}

#[test]
fn verified_launcher_and_explicit_fallback_are_distinct_persisted_scopes() {
    let sink = Arc::new(RecordingSink::default());
    let mut recorder = PersistentApplicationHistoryRecorder::new(sink.clone());

    let report = recorder.record_process_snapshot(
        &[
            process(1, "Visible name", Some("io.example.App")),
            process(2, "worker", None),
        ],
        9,
        42,
    );

    assert_eq!(report.observed_applications, 2);
    assert_eq!(report.recorded_applications, 2);
    let records = sink
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(records.len(), 6);
    assert!(records.iter().any(|(key, _)| {
        key.application()
            .is_some_and(|identity| identity.is_verified() && identity.value() == "io.example.App")
    }));
    assert!(records.iter().any(|(key, _)| {
        key.application()
            .is_some_and(|identity| !identity.is_verified() && identity.value() == "worker")
    }));
    assert!(records.iter().all(|(_, sample)| {
        sample.revision == 9 && sample.completed_at_ms == 42 && sample.measured_at_ms == Some(42)
    }));
}

#[test]
fn zero_revision_is_rejected_without_touching_the_sink() {
    let sink = Arc::new(RecordingSink::default());
    let mut recorder = PersistentApplicationHistoryRecorder::new(sink.clone());

    let report = recorder.record_process_snapshot(&[process(1, "worker", None)], 0, 42);

    assert_eq!(report.observed_applications, 1);
    assert_eq!(report.recorded_applications, 0);
    assert!(
        sink.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty()
    );
}

#[test]
fn typed_metric_history_skips_non_current_values_but_keeps_observed_counts() {
    let sink = Arc::new(RecordingSink::default());
    let mut recorder = PersistentApplicationHistoryRecorder::new(sink.clone());
    let stale_cpu = ScalarObservation::available(4.0, 41).transition_failure(FailureKind::TimedOut);

    let processes = vec![
        process_with_metrics(
            1,
            "measured-zero",
            None,
            ScalarObservation::available(0.0, 42),
            ScalarObservation::available(0, 42),
        ),
        process_with_metrics(
            2,
            "partial",
            None,
            ScalarObservation::available(2.0, 42),
            ScalarObservation::available(10, 42),
        ),
        process_with_metrics(
            3,
            "partial",
            None,
            ScalarObservation::unavailable(FailureKind::PermissionDenied),
            ScalarObservation::available(20, 42),
        ),
        process_with_metrics(
            4,
            "stale",
            None,
            stale_cpu,
            ScalarObservation::available(40, 42),
        ),
        process_with_metrics(
            5,
            "unavailable",
            None,
            ScalarObservation::unavailable(FailureKind::Unsupported),
            ScalarObservation::unavailable(FailureKind::Unsupported),
        ),
        process_with_metrics(
            6,
            "unknown",
            None,
            ScalarObservation::default(),
            ScalarObservation::default(),
        ),
    ];

    let report = recorder.record_process_snapshot(&processes, 3, 42);
    assert_eq!(report.observed_applications, 5);
    assert_eq!(report.recorded_applications, 5);

    assert_eq!(
        records_for_metric(
            &sink,
            HistoryMetric::ApplicationCpuUsagePct,
            "measured-zero"
        )
        .first()
        .and_then(|sample| sample.value),
        Some(0.0)
    );
    assert_eq!(
        records_for_metric(
            &sink,
            HistoryMetric::ApplicationMemoryBytes,
            "measured-zero"
        )
        .first()
        .and_then(|sample| sample.value),
        Some(0.0)
    );

    // Partial data retains its current measured value; it is never converted
    // into a fabricated zero. The member count is independently recorded.
    assert_eq!(
        records_for_metric(&sink, HistoryMetric::ApplicationCpuUsagePct, "partial")
            .first()
            .and_then(|sample| sample.value),
        Some(2.0)
    );
    assert_eq!(
        records_for_metric(&sink, HistoryMetric::ApplicationMemoryBytes, "partial")
            .first()
            .and_then(|sample| sample.value),
        Some(30.0)
    );
    assert_eq!(
        records_for_metric(&sink, HistoryMetric::ApplicationProcessCount, "partial")
            .first()
            .and_then(|sample| sample.value),
        Some(2.0)
    );

    // Stale/unavailable/unknown metrics have no current value, so only their
    // observed process count is persisted.
    assert!(records_for_metric(&sink, HistoryMetric::ApplicationCpuUsagePct, "stale").is_empty());
    assert!(
        records_for_metric(&sink, HistoryMetric::ApplicationCpuUsagePct, "unavailable").is_empty()
    );
    assert!(records_for_metric(&sink, HistoryMetric::ApplicationMemoryBytes, "unknown").is_empty());
    for identity in ["stale", "unavailable", "unknown"] {
        assert_eq!(
            records_for_metric(&sink, HistoryMetric::ApplicationProcessCount, identity)
                .first()
                .and_then(|sample| sample.value),
            Some(1.0)
        );
    }
}

#[test]
fn identities_are_bounded_per_snapshot_in_deterministic_verified_first_order() {
    let first_sink = Arc::new(RecordingSink::default());
    let second_sink = Arc::new(RecordingSink::default());
    let processes = (0..=MAX_PERSISTED_APPLICATION_IDENTITIES)
        .map(|index| {
            process(
                u32::try_from(index + 1).expect("bounded pid"),
                &format!("app-{index:03}"),
                (index == MAX_PERSISTED_APPLICATION_IDENTITIES).then_some("verified.last"),
            )
        })
        .collect::<Vec<_>>();
    let mut reversed = processes.clone();
    reversed.reverse();
    let mut first = PersistentApplicationHistoryRecorder::new(first_sink.clone());
    let mut second = PersistentApplicationHistoryRecorder::new(second_sink.clone());

    let first_report = first.record_process_snapshot(&processes, 1, 100);
    let second_report = second.record_process_snapshot(&reversed, 1, 100);

    assert_eq!(
        first_report.recorded_applications,
        MAX_PERSISTED_APPLICATION_IDENTITIES
    );
    assert_eq!(first_report.rejected_identity_capacity, 1);
    assert_eq!(first_report, second_report);
    let keys = |sink: &RecordingSink| {
        sink.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&first_sink), keys(&second_sink));
    assert!(keys(&first_sink).iter().any(|key| {
        key.application()
            .is_some_and(|identity| identity.value() == "verified.last")
    }));
}

#[test]
fn a_departed_snapshot_identity_never_reserves_capacity_for_later_ticks() {
    let sink = Arc::new(RecordingSink::default());
    let mut recorder = PersistentApplicationHistoryRecorder::new(sink.clone());
    let full = (0..MAX_PERSISTED_APPLICATION_IDENTITIES)
        .map(|index| {
            process(
                u32::try_from(index + 1).expect("bounded pid"),
                &format!("old-{index:03}"),
                None,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        recorder
            .record_process_snapshot(&full, 1, 100)
            .recorded_applications,
        MAX_PERSISTED_APPLICATION_IDENTITIES
    );

    let report = recorder.record_process_snapshot(&[process(999, "new-app", None)], 2, 200);

    assert_eq!(report.recorded_applications, 1);
    assert_eq!(report.rejected_identity_capacity, 0);
    assert!(
        sink.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .any(|(key, sample)| {
                sample.revision == 2
                    && key
                        .application()
                        .is_some_and(|identity| identity.value() == "new-app")
            })
    );
}
