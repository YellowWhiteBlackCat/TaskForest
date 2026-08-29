use std::thread;
use std::time::Duration;

use taskmanager_application::{PlatformEvent, SensorEvent, SensorRequest};
use taskmanager_core::core::identity::ProviderId;
use taskmanager_platform_contract::{
    CapabilityId, CapabilityStatus, DeviceDiscovery, RequestEnvelope, RequestId,
};

use super::*;
use crate::{ProviderBinding, RuntimeConfig, RuntimeProviderBindings};

const CLOCK_MS: u64 = 31;

fn fixed_clock() -> u64 {
    CLOCK_MS
}

fn sensor_bindings() -> RuntimeProviderBindings {
    let mut bindings = RuntimeProviderBindings::default();
    bindings.sensor.observation = ProviderBinding::present(ProviderId::borrowed("fixture.sensor"));
    bindings
}

#[test]
fn sensor_completion_is_independent_from_power_availability() {
    let runtime = crate::ChannelRuntime::new(sensor_bindings(), RuntimeConfig::new(fixed_clock));

    assert_eq!(runtime.lanes.sensor.missing_capabilities().count(), 0);
    assert!(runtime.lanes.power.supplies_rx.is_none());
    assert!(runtime.lanes.sensor.try_complete().is_some());
}

#[test]
fn shared_sensor_runtime_injects_clock_and_derives_source_health() {
    let runtime = crate::ChannelRuntime::new(sensor_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_sensor_lanes(
        &workers,
        lanes.sensor.try_complete().expect("complete sensor lane"),
        SensorExecutors::new(|observed_at_ms| {
            assert_eq!(observed_at_ms, CLOCK_MS);
            Ok(DeviceSourceSnapshot::from_discovery(
                SensorCenterSnapshot {
                    timestamp_ms: observed_at_ms,
                    ..SensorCenterSnapshot::default()
                },
                ProviderId::borrowed("fixture.sensor.discovery"),
                DeviceDiscovery::Empty,
                vec![],
            ))
        }),
        publisher,
        fixed_clock,
    )
    .expect("sensor worker starts");
    handle
        .sensors()
        .expect("sensor port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(1).expect("fixture request id"),
            capability: CapabilityId::SENSORS,
            submitted_at_ms: 1,
            payload: SensorRequest::Refresh,
        })
        .expect("sensor request accepted");

    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("connected event port") {
            assert_eq!(event.provider, Some(ProviderId::borrowed("fixture.sensor")));
            assert!(matches!(
                event.outcome,
                Ok(PlatformEvent::Sensors(SensorEvent::Snapshot(ref snapshot)))
                    if snapshot.value.timestamp_ms == CLOCK_MS
            ));
            assert_eq!(
                handle
                    .capabilities()
                    .snapshot()
                    .get(&CapabilityId::SENSORS)
                    .map(|descriptor| descriptor.status),
                Some(CapabilityStatus::Available)
            );
            assert_eq!(
                handle
                    .capabilities()
                    .snapshot()
                    .get(&CapabilityId::SENSORS)
                    .map(|descriptor| descriptor.providers.clone()),
                Some(vec![ProviderId::borrowed("fixture.sensor")])
            );
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("sensor runtime event did not arrive");
}
