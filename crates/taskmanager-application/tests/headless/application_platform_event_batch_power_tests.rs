use std::collections::HashMap;

use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::{
    BatteryInfo, BatteryScalarObservations, DeviceGeneration, DeviceLifecycle, DevicePresence,
    PowerSupplyKind, PowerSupplySnapshot, ScalarObservation,
};
use taskmanager_platform_contract::{
    CapabilityId, DeviceDiscovery, DeviceSourceSnapshot, RequestId,
};

use super::super::super::{PlatformEvent, PowerSupplyEvent};
use super::super::{PlatformEventBatch, test_support::test_event_context};

#[test]
fn power_supply_event_preserves_devices_and_source_status() {
    let mut batch = PlatformEventBatch::default();
    let request_id = RequestId::new(4).expect("non-zero fixture request");
    let mut battery = BatteryInfo::new(
        "power-supply:serial-a",
        taskmanager_core::DeviceState::default(),
    );
    battery.kind = PowerSupplyKind::UninterruptiblePowerSupply;
    battery.device_generation = DeviceGeneration::new(3);
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(73, 42),
        ..Default::default()
    });
    batch.merge(
        test_event_context(request_id, CapabilityId::POWER_SUPPLIES),
        PlatformEvent::PowerSupplies(PowerSupplyEvent::Snapshot(
            DeviceSourceSnapshot::from_discovery(
                PowerSupplySnapshot {
                    timestamp_ms: 42,
                    batteries: vec![battery],
                    device_lifecycles: HashMap::from([(
                        "power-supply:serial-a".to_string(),
                        DeviceLifecycle {
                            presence: DevicePresence::Present,
                            generation: 3,
                            ..Default::default()
                        },
                    )]),
                    ..Default::default()
                },
                ProviderId::borrowed("fixture.power-supply"),
                DeviceDiscovery::Available(vec![taskmanager_core::DeviceId::new(
                    "power-supply:serial-a",
                )]),
                Vec::new(),
            ),
        )),
    );

    let event = batch
        .power_supply_events
        .first()
        .expect("power-supply event should be retained");
    assert_eq!(event.request_id, request_id);
    let PowerSupplyEvent::Snapshot(snapshot) = &event.event;
    assert_eq!(snapshot.value.batteries[0].current_capacity_pct(), Some(73));
    assert_eq!(
        snapshot.value.batteries[0].kind,
        PowerSupplyKind::UninterruptiblePowerSupply
    );
    assert_eq!(snapshot.value.batteries[0].device_generation.get(), 3);
    assert_eq!(
        snapshot
            .value
            .device_lifecycles
            .get("power-supply:serial-a")
            .map(|lifecycle| lifecycle.generation),
        Some(3)
    );
    assert_eq!(
        snapshot.discovery().provider.as_str(),
        "fixture.power-supply"
    );
}
