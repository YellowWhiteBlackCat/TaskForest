use super::CpuHistoryCache;
use crate::core::CpuTelemetryObservation;
use crate::core::metrics::{CpuMetrics, CpuScalarObservations, ScalarObservation};
use std::rc::Rc;
use taskmanager_telemetry_store::CorrelatedTelemetryStamp;
use taskmanager_telemetry_store::TelemetryStore;

#[test]
fn aggregate_series_reuse_the_same_rc_until_the_generation_bumps() {
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(600);
    for (revision, usage) in [(1, 25.0), (2, 40.0)] {
        let stamp = CorrelatedTelemetryStamp::from_accepted_event(revision, revision * 1000)
            .expect("non-zero revision");
        let observation = CpuTelemetryObservation::current(
            CpuMetrics::from_observations(CpuScalarObservations {
                global_usage_pct: ScalarObservation::available(usage, revision * 1_000),
                ..Default::default()
            }),
            revision * 1000,
            Vec::new(),
        );
        ingestor
            .ingest_correlated_cpu(stamp, &observation)
            .expect("cpu observation ingests");
    }

    let mut cache = CpuHistoryCache::new();
    let first = cache.aggregate(&store);
    assert_eq!(&*first.usage, &[25.0, 40.0]);

    let second = cache.aggregate(&store);
    assert!(
        Rc::ptr_eq(&first.usage, &second.usage),
        "an unchanged generation must reuse the cached aggregate series"
    );

    let stamp = CorrelatedTelemetryStamp::from_accepted_event(3, 3000).expect("non-zero revision");
    ingestor
        .ingest_correlated_cpu(
            stamp,
            &CpuTelemetryObservation::current(
                CpuMetrics::from_observations(CpuScalarObservations {
                    global_usage_pct: ScalarObservation::available(10.0, 3_000),
                    ..Default::default()
                }),
                3000,
                Vec::new(),
            ),
        )
        .expect("cpu observation ingests");
    cache.bump();
    let third = cache.aggregate(&store);
    assert!(!Rc::ptr_eq(&first.usage, &third.usage));
    assert_eq!(&*third.usage, &[25.0, 40.0, 10.0]);
}
