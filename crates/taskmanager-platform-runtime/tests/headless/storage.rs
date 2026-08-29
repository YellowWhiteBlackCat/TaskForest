use std::thread;
use std::time::Duration;

use taskmanager_application::{
    PlatformEvent, PlatformHandle, SmartControlRequest, SmartEvent, SmartObservationRequest,
    StorageHealthEvent, StorageHealthRequest,
};
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId};
use taskmanager_core::core::smart::{SmartSelfTestKind, SmartSelfTestPhase, SmartSelfTestReport};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::core::storage_health::FilesystemHealthSnapshot;
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_platform_contract::{
    CapabilityId, CapabilityStatus, CompositeSourceSnapshot, EventEnvelope, RequestEnvelope,
    RequestId,
};

use super::*;
use crate::{ProviderBinding, RuntimeConfig, RuntimeProviderBindings};

const CLOCK_MS: u64 = 41;

fn fixed_clock() -> u64 {
    CLOCK_MS
}

fn storage_bindings() -> RuntimeProviderBindings {
    let mut bindings = RuntimeProviderBindings::default();
    bindings.storage.health =
        ProviderBinding::present(ProviderId::borrowed("fixture.storage.health"));
    bindings.storage.smart_observation =
        ProviderBinding::present(ProviderId::borrowed("fixture.storage.smart-observation"));
    bindings.storage.smart_control =
        ProviderBinding::present(ProviderId::borrowed("fixture.storage.smart-control"));
    bindings
}

fn wait_event(handle: &PlatformHandle) -> EventEnvelope<PlatformEvent> {
    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("connected event port") {
            return event;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("storage runtime event did not arrive");
}

#[test]
fn pending_storage_group_promotes_atomically_and_keeps_exact_capability_errors() {
    let complete = crate::ChannelRuntime::new(storage_bindings(), RuntimeConfig::new(fixed_clock));
    assert!(!complete.lanes.storage.health_capability_missing());
    assert_eq!(
        complete.lanes.storage.missing_smart_capabilities().count(),
        0
    );
    assert!(complete.lanes.storage.try_complete().is_some());

    let mut incomplete_bindings = storage_bindings();
    incomplete_bindings.storage.smart_control = ProviderBinding::absent();
    let incomplete =
        crate::ChannelRuntime::new(incomplete_bindings, RuntimeConfig::new(fixed_clock));
    assert_eq!(
        incomplete
            .lanes
            .storage
            .missing_smart_capabilities()
            .collect::<Vec<_>>(),
        [CapabilityId::SMART_CONTROL]
    );
    assert!(incomplete.lanes.storage.try_complete().is_none());
}

#[test]
fn shared_storage_runtime_owns_filesystem_and_smart_request_event_policy() {
    let runtime = crate::ChannelRuntime::new(storage_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
        ..
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_storage_lanes(
        &workers,
        lanes
            .storage
            .try_complete()
            .expect("complete storage lanes"),
        StorageExecutors::new(
            |observed_at_ms| {
                assert_eq!(observed_at_ms, CLOCK_MS);
                Ok(CompositeSourceSnapshot::new(
                    FilesystemHealthSnapshot::default(),
                    vec![SourceStatus {
                        provider: ProviderId::borrowed("fixture.filesystem"),
                        outcome: SourceOutcome::Empty,
                        item_count: 0,
                    }],
                ))
            },
            |target, previous, observed_at_ms| {
                assert_eq!(target.locator.as_str(), "fixture-disk");
                assert_eq!(previous, DeviceState::healthy(CLOCK_MS));
                Ok(SmartSelfTestReport {
                    state: DeviceState::healthy(observed_at_ms),
                    phase: SmartSelfTestPhase::Completed,
                    kind: Some(SmartSelfTestKind::Short),
                    ..SmartSelfTestReport::default()
                })
            },
            |intent, observed_at_ms| {
                assert_eq!(intent.device_key.as_str(), "fixture-disk");
                Ok(SmartSelfTestReport {
                    state: DeviceState::healthy(observed_at_ms),
                    phase: SmartSelfTestPhase::Running,
                    kind: Some(intent.kind),
                    ..SmartSelfTestReport::default()
                })
            },
        ),
        publisher,
        fixed_clock,
    )
    .expect("storage workers start");

    handle
        .storage_health()
        .expect("storage health port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(1).expect("fixture request id"),
            capability: CapabilityId::STORAGE_HEALTH,
            submitted_at_ms: 1,
            payload: StorageHealthRequest::Refresh,
        })
        .expect("storage health request accepted");
    let health_event = wait_event(&handle);
    assert_eq!(
        health_event.provider,
        Some(ProviderId::borrowed("fixture.storage.health"))
    );
    assert!(matches!(
        health_event.outcome,
        Ok(PlatformEvent::StorageHealth(StorageHealthEvent::Snapshot(
            _
        )))
    ));

    let intent = SmartSelfTestIntent {
        device_id: DeviceId::new("disk:fixture"),
        device_generation: DeviceGeneration::INITIAL,
        device_key: "fixture-disk".into(),
        display_name: "Fixture disk".into(),
        kind: SmartSelfTestKind::Short,
    };
    let target = intent.target();
    handle
        .smart_control()
        .expect("SMART control port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(2).expect("fixture request id"),
            capability: CapabilityId::SMART_CONTROL,
            submitted_at_ms: 2,
            payload: SmartControlRequest::StartSelfTest(intent),
        })
        .expect("SMART control request accepted");
    let control_event = wait_event(&handle);
    assert_eq!(
        control_event.provider,
        Some(ProviderId::borrowed("fixture.storage.smart-control"))
    );
    assert!(matches!(
        control_event.outcome,
        Ok(PlatformEvent::Smart(SmartEvent::Batch(ref batch)))
            if batch.revision.get() == 1
                && batch.observations.len() == 1
                && batch.observations[0].report.phase == SmartSelfTestPhase::Running
    ));

    handle
        .smart_observation()
        .expect("SMART observation port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(3).expect("fixture request id"),
            capability: CapabilityId::SMART,
            submitted_at_ms: 3,
            payload: SmartObservationRequest::RefreshTarget(target),
        })
        .expect("SMART observation request accepted");
    let observation_event = wait_event(&handle);
    assert_eq!(
        observation_event.provider,
        Some(ProviderId::borrowed("fixture.storage.smart-observation"))
    );
    assert!(matches!(
        observation_event.outcome,
        Ok(PlatformEvent::Smart(SmartEvent::Batch(ref batch)))
            if batch.revision.get() == 2
                && batch.observations.len() == 1
                && batch.observations[0].report.phase == SmartSelfTestPhase::Completed
    ));
    assert_eq!(
        handle
            .capabilities()
            .snapshot()
            .get(&CapabilityId::SMART)
            .map(|descriptor| descriptor.status),
        Some(CapabilityStatus::Available)
    );
    assert_eq!(
        handle
            .capabilities()
            .snapshot()
            .get(&CapabilityId::SMART)
            .map(|descriptor| descriptor.providers.clone()),
        Some(vec![ProviderId::borrowed(
            "fixture.storage.smart-observation"
        )])
    );
    assert_eq!(
        handle
            .capabilities()
            .snapshot()
            .get(&CapabilityId::SMART_CONTROL)
            .map(|descriptor| descriptor.providers.clone()),
        Some(vec![ProviderId::borrowed("fixture.storage.smart-control")])
    );
}
