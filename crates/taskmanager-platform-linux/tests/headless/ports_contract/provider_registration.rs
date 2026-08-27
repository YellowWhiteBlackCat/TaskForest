//! Catalog and event identity contracts for typed provider registrations.

use super::*;

pub(super) fn fixture_service_provider(capability: &CapabilityId) -> ProviderId {
    if capability == &CapabilityId::SERVICES {
        ProviderId::borrowed("fixture.service.inventory")
    } else if capability == &CapabilityId::SERVICE_DEPENDENCIES {
        ProviderId::borrowed("fixture.service.dependencies")
    } else if capability == &CapabilityId::SERVICE_CONTROL {
        ProviderId::borrowed("fixture.service.control")
    } else if capability == &CapabilityId::SERVICE_LOGS {
        ProviderId::borrowed("fixture.service.log-snapshot")
    } else if capability == &CapabilityId::SERVICE_LOG_STREAM {
        ProviderId::borrowed("fixture.service.log-stream")
    } else {
        panic!("unexpected service capability {capability}");
    }
}

pub(super) fn fixture_environment_provider(capability: &CapabilityId) -> ProviderId {
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

pub(super) fn fixture_system_provider(capability: &CapabilityId) -> ProviderId {
    if capability == &CapabilityId::TELEMETRY_HOST {
        ProviderId::borrowed("fixture.system.host")
    } else if capability == &CapabilityId::TELEMETRY_CPU {
        ProviderId::borrowed("fixture.system.cpu")
    } else if capability == &CapabilityId::TELEMETRY_MEMORY {
        ProviderId::borrowed("fixture.system.memory")
    } else if capability == &CapabilityId::TELEMETRY_STORAGE {
        ProviderId::borrowed("fixture.system.storage")
    } else if capability == &CapabilityId::TELEMETRY_NETWORK {
        ProviderId::borrowed("fixture.system.network")
    } else if capability == &CapabilityId::TELEMETRY_GPU {
        ProviderId::borrowed("fixture.system.gpu")
    } else if capability == &CapabilityId::HARDWARE_INVENTORY {
        ProviderId::borrowed("fixture.system.hardware-inventory")
    } else {
        panic!("unexpected system capability {capability}");
    }
}

pub(super) fn fixture_process_provider(capability: &CapabilityId) -> ProviderId {
    if capability == &CapabilityId::PROCESS_LIST {
        ProviderId::borrowed("fixture.process.list")
    } else if capability == &CapabilityId::PROCESS_CONTROL {
        ProviderId::borrowed("fixture.process.control")
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_NETWORK {
        ProviderId::borrowed("fixture.process.network")
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_GPU {
        ProviderId::borrowed("fixture.process.gpu")
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_RESOURCES {
        ProviderId::borrowed("fixture.process.resources")
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_ISOLATION {
        ProviderId::borrowed("fixture.process.isolation")
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_THREADS {
        ProviderId::borrowed("fixture.process.threads")
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_ENVIRONMENT {
        ProviderId::borrowed("fixture.process.environment")
    } else if capability == &CapabilityId::PROCESS_AFFINITY {
        ProviderId::borrowed("fixture.process.affinity")
    } else if capability == &CapabilityId::PROCESS_AFFINITY_CONTROL {
        ProviderId::borrowed("fixture.process.affinity-control")
    } else if capability == &CapabilityId::PROCESS_RESOURCE_CONTROL {
        ProviderId::borrowed("fixture.process.resource-control")
    } else if capability == &CapabilityId::PROCESS_NETWORK_ESCALATION {
        ProviderId::borrowed("fixture.process.network-escalation")
    } else {
        panic!("unexpected process capability {capability}");
    }
}

#[test]
fn system_catalog_keeps_seven_distinct_registration_identities() {
    let handle = spawn_complete(fake_registry(FakeProvider::default()));
    let capabilities = handle.capabilities().snapshot();

    for capability in [
        CapabilityId::TELEMETRY_HOST,
        CapabilityId::TELEMETRY_CPU,
        CapabilityId::TELEMETRY_MEMORY,
        CapabilityId::TELEMETRY_STORAGE,
        CapabilityId::TELEMETRY_NETWORK,
        CapabilityId::TELEMETRY_GPU,
        CapabilityId::HARDWARE_INVENTORY,
    ] {
        assert_eq!(
            capabilities
                .get(&capability)
                .map(|descriptor| descriptor.providers.clone()),
            Some(vec![fixture_system_provider(&capability)])
        );
    }
}

#[test]
fn process_catalog_keeps_eleven_distinct_registration_identities() {
    let handle = spawn_complete(fake_registry(FakeProvider::default()));
    let capabilities = handle.capabilities().snapshot();

    for capability in [
        CapabilityId::PROCESS_LIST,
        CapabilityId::PROCESS_CONTROL,
        CapabilityId::PROCESS_INSIGHTS_NETWORK,
        CapabilityId::PROCESS_INSIGHTS_GPU,
        CapabilityId::PROCESS_INSIGHTS_RESOURCES,
        CapabilityId::PROCESS_INSIGHTS_ISOLATION,
        CapabilityId::PROCESS_INSIGHTS_THREADS,
        CapabilityId::PROCESS_AFFINITY,
        CapabilityId::PROCESS_AFFINITY_CONTROL,
        CapabilityId::PROCESS_RESOURCE_CONTROL,
        CapabilityId::PROCESS_NETWORK_ESCALATION,
    ] {
        assert_eq!(
            capabilities
                .get(&capability)
                .map(|descriptor| descriptor.providers.clone()),
            Some(vec![fixture_process_provider(&capability)])
        );
    }
}

#[test]
fn process_request_rejects_a_mismatched_capability() {
    let handle = spawn_complete(fake_registry(FakeProvider::default()));
    let mut ids = RequestIdGenerator::default();
    let error = handle
        .process_list()
        .expect("process list facet")
        .try_submit(RequestEnvelope {
            id: ids.next_id(),
            capability: CapabilityId::SERVICES,
            submitted_at_ms: 1,
            payload: ProcessListRequest::Refresh,
        })
        .expect_err("mismatched capability must be rejected");

    assert_eq!(error.kind, SubmissionErrorKind::InvalidRequest);
}

#[test]
fn process_list_event_uses_its_registered_provider_identity() {
    let handle = spawn_complete(fake_registry(FakeProvider::default()));
    let mut ids = RequestIdGenerator::default();
    let request_id = submit_process_list(
        &handle,
        &mut ids,
        CapabilityId::PROCESS_LIST,
        ProcessListRequest::Refresh,
    );

    let event = wait_event(&handle);
    assert_eq!(event.request_id, request_id);
    assert_eq!(event.capability, CapabilityId::PROCESS_LIST);
    assert_eq!(
        event.provider,
        Some(fixture_process_provider(&event.capability))
    );
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::Processes(ProcessEvent::Snapshot(ref processes)))
            if processes.first().is_some_and(|process| process.pid == 42)
    ));
    assert_eq!(
        handle
            .capabilities()
            .snapshot()
            .get(&CapabilityId::PROCESS_LIST)
            .map(|descriptor| descriptor.status),
        Some(CapabilityStatus::Available)
    );
}

#[test]
fn process_network_event_keeps_identity_and_frozen_target() {
    let provider = FakeProvider::default();
    let observed_targets = provider.process_telemetry_targets.clone();
    let handle = spawn_complete(fake_registry(provider));
    let mut ids = RequestIdGenerator::default();
    let request_id = ids.next_id();
    let target = frozen_process(42);
    handle
        .process_network()
        .expect("process network facet")
        .try_submit(RequestEnvelope {
            id: request_id,
            capability: CapabilityId::PROCESS_INSIGHTS_NETWORK,
            submitted_at_ms: 1,
            payload: ProcessNetworkRequest {
                target: target.clone(),
                revision: ProcessInsightsRevision::new(1),
            },
        })
        .expect("process insights request accepted");

    let event = wait_event(&handle);
    assert_eq!(event.request_id, request_id);
    assert_eq!(
        event.provider,
        Some(fixture_process_provider(&event.capability))
    );
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::ProcessInsightFacet(
            ProcessInsightFacetEvent::Network(ref observation)
        )) if observation.snapshot.identity.pid == 42
    ));
    assert_eq!(
        observed_targets
            .lock()
            .expect("observed targets")
            .as_slice(),
        [target],
        "the frozen process identity must reach the provider unchanged"
    );
}
