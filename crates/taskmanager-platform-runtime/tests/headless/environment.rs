use std::thread;
use std::time::Duration;

use taskmanager_application::{
    LatestControlRequest, PlatformEvent, PlatformHandle, SessionControlRequest, SessionEvent,
    StartupEvent, StartupEvidenceEvent, StartupEvidenceRequest, StartupInventoryRequest,
};
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::core::session::SessionControlAction;
use taskmanager_core::core::startup::StartupBootEvidenceSnapshot;
use taskmanager_platform_contract::{
    CapabilityId, EventEnvelope, PartialSourceSnapshot, RequestEnvelope, RequestId,
};

use super::*;
use crate::{ProviderBinding, RuntimeConfig, RuntimeProviderBindings};

fn fixed_clock() -> u64 {
    29
}

fn environment_bindings() -> RuntimeProviderBindings {
    let mut bindings = RuntimeProviderBindings::default();
    bindings.environment.startup_inventory = ProviderBinding::present(ProviderId::borrowed(
        "fixture.environment.startup-inventory",
    ));
    bindings.environment.startup_evidence =
        ProviderBinding::present(ProviderId::borrowed("fixture.environment.startup-evidence"));
    bindings.environment.startup_control =
        ProviderBinding::present(ProviderId::borrowed("fixture.environment.startup-control"));
    bindings.environment.session_inventory = ProviderBinding::present(ProviderId::borrowed(
        "fixture.environment.session-inventory",
    ));
    bindings.environment.session_control =
        ProviderBinding::present(ProviderId::borrowed("fixture.environment.session-control"));
    bindings
}

fn registered_environment_provider(capability: &CapabilityId) -> ProviderId {
    if capability == &CapabilityId::STARTUP {
        ProviderId::borrowed("fixture.environment.startup-inventory")
    } else if capability == &CapabilityId::STARTUP_EVIDENCE {
        ProviderId::borrowed("fixture.environment.startup-evidence")
    } else if capability == &CapabilityId::STARTUP_CONTROL {
        ProviderId::borrowed("fixture.environment.startup-control")
    } else if capability == &CapabilityId::SESSIONS {
        ProviderId::borrowed("fixture.environment.session-inventory")
    } else if capability == &CapabilityId::SESSION_CONTROL {
        ProviderId::borrowed("fixture.environment.session-control")
    } else {
        panic!("unexpected environment capability {capability}");
    }
}

fn wait_event(handle: &PlatformHandle) -> EventEnvelope<PlatformEvent> {
    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("connected event port") {
            return event;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("environment runtime event did not arrive");
}

#[test]
fn environment_catalog_keeps_five_distinct_registered_provider_identities() {
    let runtime =
        crate::ChannelRuntime::new(environment_bindings(), RuntimeConfig::new(fixed_clock));
    let capabilities = runtime.handle.capabilities().snapshot();

    for capability in [
        CapabilityId::STARTUP,
        CapabilityId::STARTUP_EVIDENCE,
        CapabilityId::STARTUP_CONTROL,
        CapabilityId::SESSIONS,
        CapabilityId::SESSION_CONTROL,
    ] {
        assert_eq!(
            capabilities
                .get(&capability)
                .map(|descriptor| descriptor.providers.clone()),
            Some(vec![registered_environment_provider(&capability)])
        );
    }
}

#[test]
fn pending_environment_group_promotes_atomically_and_reports_one_missing_lane() {
    let complete =
        crate::ChannelRuntime::new(environment_bindings(), RuntimeConfig::new(fixed_clock));
    assert_eq!(complete.lanes.environment.missing_capabilities().count(), 0);
    assert!(complete.lanes.environment.try_complete().is_some());

    let mut incomplete_bindings = environment_bindings();
    incomplete_bindings.environment.session_control = ProviderBinding::absent();
    let incomplete =
        crate::ChannelRuntime::new(incomplete_bindings, RuntimeConfig::new(fixed_clock));
    assert_eq!(
        incomplete
            .lanes
            .environment
            .missing_capabilities()
            .collect::<Vec<_>>(),
        [CapabilityId::SESSION_CONTROL]
    );
    assert!(incomplete.lanes.environment.try_complete().is_none());
}

#[test]
fn shared_environment_runtime_preserves_typed_control_outcome() {
    let runtime =
        crate::ChannelRuntime::new(environment_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_environment_lanes(
        &workers,
        lanes
            .environment
            .try_complete()
            .expect("complete environment lanes"),
        EnvironmentExecutors::new(
            || Err(ProviderFailure::Unsupported),
            |_observed_at_ms| Err(ProviderFailure::Unsupported),
            |_entry, _enabled| Err(ProviderFailure::Unsupported),
            || Err(ProviderFailure::Unsupported),
            |_session_id, _action| Err(ProviderFailure::PermissionDenied),
        ),
        publisher,
        fixed_clock,
    )
    .expect("environment workers start");
    let mut controls = LatestControlRequest::default();
    let control_id = controls.begin();
    handle
        .session_control()
        .expect("session control port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(1).expect("fixture request id"),
            capability: CapabilityId::SESSION_CONTROL,
            submitted_at_ms: 1,
            payload: SessionControlRequest {
                request_id: control_id,
                session_id: SessionId::new("fixture-session"),
                action: SessionControlAction::Lock,
            },
        })
        .expect("session control request accepted");

    let event = wait_event(&handle);
    assert_eq!(
        event.provider,
        Some(registered_environment_provider(&event.capability))
    );
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::Sessions(SessionEvent::Control(ref outcome)))
            if outcome.request_id == control_id
                && outcome.result == Err(FailureKind::PermissionDenied)
    ));
}

#[test]
fn startup_evidence_is_correlated_on_its_own_capability() {
    let runtime =
        crate::ChannelRuntime::new(environment_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_environment_lanes(
        &workers,
        lanes
            .environment
            .try_complete()
            .expect("complete environment lanes"),
        EnvironmentExecutors::new(
            || Err(ProviderFailure::Unsupported),
            |observed_at_ms| {
                assert_eq!(observed_at_ms, fixed_clock());
                let state = DeviceState::healthy(29);
                Ok(StartupBootEvidenceSnapshot {
                    state,
                    failed_units_state: state,
                    critical_chain_state: state,
                    ..StartupBootEvidenceSnapshot::default()
                })
            },
            |_entry, _enabled| Err(ProviderFailure::Unsupported),
            || Err(ProviderFailure::Unsupported),
            |_session_id, _action| Err(ProviderFailure::Unsupported),
        ),
        publisher,
        fixed_clock,
    )
    .expect("environment workers start");
    handle
        .startup_evidence()
        .expect("startup evidence port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(7).expect("fixture request id"),
            capability: CapabilityId::STARTUP_EVIDENCE,
            submitted_at_ms: 1,
            payload: StartupEvidenceRequest::Refresh,
        })
        .expect("evidence request accepted");

    let event = wait_event(&handle);
    assert_eq!(event.request_id.get(), 7);
    assert_eq!(event.capability, CapabilityId::STARTUP_EVIDENCE);
    assert_eq!(
        event.provider,
        Some(registered_environment_provider(&event.capability))
    );
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::StartupEvidence(
            StartupEvidenceEvent::Snapshot(_)
        ))
    ));
}

#[test]
fn blocked_startup_evidence_never_stalls_startup_inventory_lane() {
    let runtime =
        crate::ChannelRuntime::new(environment_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
    } = runtime;
    let (release_tx, release_rx) = crossbeam_channel::bounded::<()>(1);
    let workers = crate::WorkerRuntime::default();
    spawn_environment_lanes(
        &workers,
        lanes
            .environment
            .try_complete()
            .expect("complete environment lanes"),
        EnvironmentExecutors::new(
            || Ok(PartialSourceSnapshot::new(Vec::new(), Vec::new())),
            move |_observed_at_ms| {
                release_rx
                    .recv()
                    .map_err(|_| ProviderFailure::TemporarilyUnavailable)?;
                Ok(StartupBootEvidenceSnapshot::default())
            },
            |_entry, _enabled| Err(ProviderFailure::Unsupported),
            || Err(ProviderFailure::Unsupported),
            |_session_id, _action| Err(ProviderFailure::Unsupported),
        ),
        publisher,
        fixed_clock,
    )
    .expect("environment workers start");
    handle
        .startup_evidence()
        .expect("startup evidence port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(8).expect("fixture request id"),
            capability: CapabilityId::STARTUP_EVIDENCE,
            submitted_at_ms: 1,
            payload: StartupEvidenceRequest::Refresh,
        })
        .expect("blocking evidence request accepted");
    handle
        .startup_inventory()
        .expect("startup inventory port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(9).expect("fixture request id"),
            capability: CapabilityId::STARTUP,
            submitted_at_ms: 2,
            payload: StartupInventoryRequest::Refresh,
        })
        .expect("inventory request accepted");

    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("connected event port")
            && matches!(
                event.outcome,
                Ok(PlatformEvent::Startup(StartupEvent::Snapshot(_)))
            )
        {
            assert_eq!(event.request_id.get(), 9);
            assert_eq!(
                event.provider,
                Some(registered_environment_provider(&event.capability))
            );
            release_tx.send(()).expect("release evidence lane");
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    release_tx.send(()).ok();
    panic!("startup inventory was stalled by evidence");
}
