use super::*;
use std::sync::Mutex;
use taskmanager_core::{
    HistoricalSample, HistorySeriesKey, ProcessApplicationIdentity, ProcessMetadataObservation,
    ProcessScalarObservations, ScalarObservation,
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
    let mut process = ProcessItem::new(pid, name);
    process.apply_scalar_observations(ProcessScalarObservations {
        cpu_percentage: ScalarObservation::available(7.5, 42),
        memory_bytes: ScalarObservation::available(512, 42),
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
