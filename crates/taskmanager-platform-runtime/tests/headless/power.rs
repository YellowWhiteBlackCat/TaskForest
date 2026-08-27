use std::thread;
use std::time::Duration;

use taskmanager_application::{
    CapabilityId, CapabilityStatus, FailureKind, PowerSupplyRequest, ProviderFailure, ProviderId,
    RequestEnvelope, RequestId,
};

use super::*;
use crate::{ProviderBinding, RuntimeConfig, RuntimeProviderBindings};

fn fixed_clock() -> u64 {
    37
}

fn power_bindings() -> RuntimeProviderBindings {
    let mut bindings = RuntimeProviderBindings::default();
    bindings.power.supplies = ProviderBinding::present(ProviderId::borrowed("fixture.power"));
    bindings
}

#[test]
fn power_completion_is_independent_from_sensor_availability() {
    let runtime = crate::ChannelRuntime::new(power_bindings(), RuntimeConfig::new(fixed_clock));

    assert_eq!(runtime.lanes.power.missing_capabilities().count(), 0);
    assert!(runtime.lanes.sensor.observation_rx.is_none());
    assert!(runtime.lanes.power.try_complete().is_some());
}

#[test]
fn shared_power_runtime_reports_typed_provider_failure() {
    let runtime = crate::ChannelRuntime::new(power_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_power_lanes(
        &workers,
        lanes.power.try_complete().expect("complete power lane"),
        PowerExecutors::new(|_observed_at_ms| Err(ProviderFailure::Unsupported)),
        publisher,
        fixed_clock,
    )
    .expect("power worker starts");
    handle
        .power_supplies()
        .expect("power port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(1).expect("fixture request id"),
            capability: CapabilityId::POWER_SUPPLIES,
            submitted_at_ms: 1,
            payload: PowerSupplyRequest::Refresh,
        })
        .expect("power request accepted");

    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("connected event port") {
            assert_eq!(event.provider, Some(ProviderId::borrowed("fixture.power")));
            assert!(matches!(
                event.outcome,
                Err(ref failure) if failure.kind == FailureKind::Unsupported
            ));
            assert_eq!(
                handle
                    .capabilities()
                    .snapshot()
                    .get(&CapabilityId::POWER_SUPPLIES)
                    .map(|descriptor| descriptor.status),
                Some(CapabilityStatus::Unsupported)
            );
            assert_eq!(
                handle
                    .capabilities()
                    .snapshot()
                    .get(&CapabilityId::POWER_SUPPLIES)
                    .map(|descriptor| descriptor.providers.clone()),
                Some(vec![ProviderId::borrowed("fixture.power")])
            );
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("power runtime failure did not arrive");
}
