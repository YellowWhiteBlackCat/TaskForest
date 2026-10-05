use super::super::{CaptureEvidence, CaptureScenario, SystemSnapshot, TopPage};
use super::PROCESSES_OBSERVED_AT_MS;
use taskmanager_core::core::metrics::DiskPartition;
use taskmanager_core::core::{DeviceId, PowerSupplySnapshot, SensorCenterSnapshot};
use taskmanager_telemetry_store::{HistoryRetention, TelemetryStore};
use taskmanager_test_support::DiskMetricsFixtureBuilder;

#[test]
fn dynamic_battery_capture_requires_complete_correlated_histories() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::BatteryLivePerformance));
    evidence.on_snapshot(&mut SystemSnapshot::default());
    let _ = evidence.on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut Vec::new());
    let mut page = TopPage::Apps;
    let mut power = PowerSupplySnapshot::default();
    let mut sensors = SensorCenterSnapshot::default();
    assert!(evidence.on_dynamic_device_state(&mut page, &mut power, &mut sensors));
    assert!(
        !evidence.scenario_ready(),
        "a scalar is not a painted history window"
    );
    let (store, ingestor) =
        TelemetryStore::shared_with_correlated_ingestion(HistoryRetention::uniform(32));
    assert!(evidence.seed_dynamic_capture_history(&store.system_history, &ingestor, 10_000));
    assert!(evidence.scenario_ready());
    let dynamic = store.system_history.dynamic_history();
    let id = DeviceId::new("power-supply:capture-battery");
    let charge = dynamic
        .battery_capacity_pct(&id)
        .expect("charge history")
        .samples();
    let watts = dynamic
        .battery_power_w(&id)
        .expect("power history")
        .samples();
    assert_eq!(charge.len(), 8);
    assert_eq!(watts.len(), 8);
    assert_ne!(charge.first(), charge.last());
    assert_ne!(watts.first(), watts.last());
    assert!(!evidence.on_dynamic_device_state(&mut page, &mut power, &mut sensors));
}

#[test]
fn live_partition_capture_waits_for_two_real_children_and_never_inserts_them() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::PartitionLiveUsage));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    assert!(!evidence.scenario_ready());

    let mut processes = Vec::new();
    assert!(
        evidence
            .on_processes_update(true, PROCESSES_OBSERVED_AT_MS, &mut processes)
            .is_none()
    );
    assert!(!evidence.scenario_ready());

    snapshot.disks = vec![
        DiskMetricsFixtureBuilder::new()
            .device_id("disk:real-partition-host".into())
            .partitions(vec![DiskPartition::default(), DiskPartition::default()])
            .build(),
    ];
    evidence.on_snapshot(&mut snapshot);
    assert!(evidence.scenario_ready());
    assert_eq!(snapshot.disks[0].partitions.len(), 2);
}
