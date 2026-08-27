//! Typed gap, timestamp, and terminal-failure history regressions.

use super::*;

#[test]
fn explicit_disk_scalar_failure_writes_gap_instead_of_legacy_zero_or_value() {
    let device_id = "disk:wwid:typed-gap";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(2);
    let mut disk = healthy_disk(device_id, 1, 55.0);
    let mut observations = *disk.scalar_observations();
    observations.active_time_pct = ScalarObservation::unavailable(FailureKind::PermissionDenied);
    disk.apply_scalar_observations(observations);
    let observation = StorageTelemetryObservation::current(
        vec![disk],
        20,
        Vec::new(),
        Vec::new(),
        lifecycles(device_id, DevicePresence::Present, 1),
    );

    ingestor
        .ingest_correlated_storage(stamp(2), &observation)
        .expect("field-level failure remains a valid domain event");

    assert_eq!(
        store
            .system_history
            .storage_activity(&DeviceId::new(device_id))
            .expect("typed identity history")
            .samples()[0]
            .value,
        None
    );
}

#[test]
fn completion_and_measurement_times_stay_distinct_and_invalid_order_fails_closed() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(3);
    let cpu = CpuTelemetryObservation::current(observed_cpu(5.0, 100), 100, Vec::new());
    let slow_completion =
        CorrelatedTelemetryStamp::from_accepted_event(1, 250).expect("non-zero revision");
    ingestor
        .ingest_correlated_cpu(slow_completion, &cpu)
        .expect("completion after measurement is valid");
    let sample = &store.system_history.cpu_usage().samples()[0];
    assert_eq!(sample.measured_at_ms, Some(100));
    assert_eq!(sample.stamp.completed_at_ms(), 250);

    let impossible =
        CorrelatedTelemetryStamp::from_accepted_event(2, 99).expect("non-zero revision");
    assert_eq!(
        ingestor
            .ingest_correlated_cpu(impossible, &cpu)
            .expect_err("completion before measurement must fail closed"),
        CorrelatedIngestionError::CompletionPrecedesMeasurement {
            domain: SystemHistoryDomain::Cpu,
            measured_at_ms: 100,
            completed_at_ms: 99,
        }
    );
    assert_eq!(store.system_history.cpu_usage().samples().len(), 1);
}

#[test]
fn accepted_failure_advances_all_existing_device_series_with_gaps() {
    let disk_id = "disk:wwid:failure-gap";
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(3);
    let present = StorageTelemetryObservation::current(
        vec![healthy_disk(disk_id, 1, 8.0)],
        10,
        Vec::new(),
        Vec::new(),
        lifecycles(disk_id, DevicePresence::Present, 1),
    );
    ingestor
        .ingest_correlated_storage(stamp(1), &present)
        .expect("initial device observation");
    ingestor
        .ingest_correlated_unavailable(
            stamp(2),
            SystemHistoryDomain::Storage,
            FailureKind::TimedOut,
        )
        .expect("correlated terminal failure");

    let history = store
        .system_history
        .storage_activity(&DeviceId::new(disk_id))
        .expect("existing device history is retained");
    assert_eq!(
        history
            .samples()
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [Some(8.0), None]
    );
    let receipt = store.system_history.receipts(SystemHistoryDomain::Storage)[1];
    assert_eq!(
        receipt.state,
        SystemObservationState::Unavailable {
            failure: FailureKind::TimedOut,
        }
    );
}

#[test]
fn zero_revision_cannot_form_a_correlation_stamp() {
    assert!(CorrelatedTelemetryStamp::from_accepted_event(0, 10).is_none());
}
