use std::collections::HashMap;

use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{
    DeviceGeneration, DeviceId, DeviceLifecycle, DevicePresence, DeviceState, SensorCenterSnapshot,
    SensorDescriptor, SensorMagnitude, SensorMeasurementObservation, SensorReading, SensorScale,
};
use taskmanager_platform_contract::{CapabilityId, DeviceSourceSnapshot, RequestId};

use super::super::super::{PlatformEvent, SensorEvent};
use super::super::{PlatformEventBatch, test_support::test_event_context};

#[test]
fn sensor_event_preserves_physical_device_generation_and_lifecycle() {
    let mut batch = PlatformEventBatch::default();
    let device_id = DeviceId::new("hwmon:pci:0000:00:01.0:coretemp");
    batch.merge(
        test_event_context(
            RequestId::new(5).expect("non-zero fixture request"),
            CapabilityId::SENSORS,
        ),
        PlatformEvent::Sensors(SensorEvent::Snapshot(
            DeviceSourceSnapshot::from_source_status(
                SensorCenterSnapshot {
                    state: DeviceState::healthy(42),
                    timestamp_ms: 42,
                    readings: vec![
                        SensorReading::from_measurement_observation(
                            device_id.clone(),
                            format!("{}:temp1", device_id.as_str()),
                            "Package".into(),
                            SensorMeasurementObservation::available(
                                SensorDescriptor::temperature(SensorScale::IDENTITY),
                                SensorMagnitude::Decimal(42.0),
                                42,
                            )
                            .expect("valid temperature fixture"),
                        )
                        .with_device_generation(DeviceGeneration::new(2)),
                    ],
                    thermal_control: Default::default(),
                    device_lifecycles: HashMap::from([(
                        device_id.as_str().to_owned(),
                        DeviceLifecycle {
                            presence: DevicePresence::Present,
                            generation: 2,
                            ..Default::default()
                        },
                    )]),
                },
                vec![device_id.clone()],
                SourceStatus {
                    provider: ProviderId::borrowed("fixture.sensor"),
                    outcome: SourceOutcome::Available,
                    item_count: 1,
                },
                Vec::new(),
            ),
        )),
    );

    let event = batch
        .sensor_events
        .first()
        .expect("sensor event should be retained");
    let SensorEvent::Snapshot(snapshot) = &event.event;
    assert_eq!(snapshot.value.readings[0].device_id(), &device_id);
    assert_eq!(snapshot.value.readings[0].device_generation().get(), 2);
    assert_eq!(
        snapshot
            .value
            .device_lifecycles
            .get(device_id.as_str())
            .map(|lifecycle| lifecycle.generation),
        Some(2)
    );
}
