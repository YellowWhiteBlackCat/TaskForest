use super::super::{CaptureEvidence, CaptureScenario, SystemSnapshot, TopPage};
use taskmanager_core::core::metrics::DiskPartition;
use taskmanager_core::core::{
    BatteryInfo, BatteryScalarObservations, DeviceGeneration, DeviceState, PowerSupplySnapshot,
    ScalarObservation,
};

#[test]
fn live_battery_capture_waits_for_real_data_and_never_inserts_a_fixture() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::BatteryLivePerformance));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = Vec::new();
    assert!(evidence.on_processes_update(true, &mut processes).is_none());

    let mut page = TopPage::Apps;
    let mut power_supplies = PowerSupplySnapshot::default();
    assert!(!evidence.on_live_dynamic_device_state(&mut page, &power_supplies));
    assert!(!evidence.scenario_ready);

    let mut battery = BatteryInfo::new("power-supply:real-battery", DeviceState::healthy(100));
    battery.device_generation = DeviceGeneration::new(1);
    battery.status = "Discharging".into();
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(79, 100),
        ..Default::default()
    });
    power_supplies = PowerSupplySnapshot {
        timestamp_ms: 100,
        batteries: vec![battery],
        ..Default::default()
    };
    assert!(evidence.on_live_dynamic_device_state(&mut page, &power_supplies));
    assert_eq!(page, TopPage::Performance);
    assert!(evidence.scenario_ready);
}

#[test]
fn live_partition_capture_waits_for_two_real_children_and_never_inserts_them() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::PartitionLiveUsage));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    assert!(!evidence.scenario_ready);

    let mut processes = Vec::new();
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(!evidence.scenario_ready);

    snapshot.disks = vec![
        taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .device_id("disk:real-partition-host".into())
            .partitions(vec![DiskPartition::default(), DiskPartition::default()])
            .build(),
    ];
    evidence.on_snapshot(&mut snapshot);
    assert!(evidence.scenario_ready);
    assert_eq!(snapshot.disks[0].partitions.len(), 2);
}
