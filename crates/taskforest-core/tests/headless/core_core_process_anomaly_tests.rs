use super::*;
use crate::ProcessScalarObservations;
use crate::core::metrics::ScalarObservation;

#[test]
fn anomaly_rules_require_real_snapshot_evidence() {
    let mut zombies = Vec::new();
    for pid in 1..=ZOMBIE_STORM_THRESHOLD as u32 {
        let mut process = ProcessItem::new(pid, format!("zombie-{pid}"));
        process.status = "Z".into();
        zombies.push(process);
    }
    let anomalies = detect_process_anomalies(&zombies);
    assert!(anomalies.iter().any(|anomaly| {
        anomaly.pid.is_none() && anomaly.kind == ProcessAnomalyKind::ZombieStorm
    }));

    let mut process = ProcessItem::new(42, "leaky");
    process.apply_scalar_observations(ProcessScalarObservations {
        fds: ScalarObservation::available(FD_PRESSURE_THRESHOLD, 10),
        ..Default::default()
    });
    process.mem_history = vec![10.0, 11.0, 12.0, 13.0];
    let anomalies = detect_process_anomalies(&[process]);
    assert!(anomalies.iter().any(|anomaly| anomaly.kind
        == ProcessAnomalyKind::FileDescriptorPressure
        && anomaly.pid == Some(42)));
    assert!(anomalies.iter().any(|anomaly| anomaly.kind
        == ProcessAnomalyKind::MonotonicMemoryGrowth
        && anomaly.pid == Some(42)));
}

#[test]
fn memory_heuristic_does_not_cross_gaps_or_short_histories() {
    let mut process = ProcessItem::new(1, "unknown");
    process.mem_history = vec![10.0, f32::NAN, 30.0, 40.0];
    assert!(detect_process_anomalies(&[process]).is_empty());
}
