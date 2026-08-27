use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use taskmanager_application::{
    CapabilityId, CapabilityStatus, CommandLaunchRequest, CompositeSourceSnapshot,
    CpuTelemetryRequest, DesktopAppearanceEvent, DesktopAppearanceRequest, DeviceSourceSnapshot,
    EventEnvelope, FailureKind, HardwareInventoryRequest, LatestControlRequest,
    PartialSourceSnapshot, PlatformEvent, PlatformHandle, PowerSupplyEvent, PowerSupplyRequest,
    ProcessAffinityControlRequest, ProcessAffinityRequest, ProcessControlRequest, ProcessEvent,
    ProcessGpuRequest, ProcessInsightFacetEvent, ProcessInsightsRevision, ProcessListRequest,
    ProcessNetworkRequest, ProviderFailure, ProviderId, RequestEnvelope, RequestIdGenerator,
    ResourceRevealRequest, SensorEvent, SensorRequest, ServiceControlRequest,
    ServiceDependenciesRequest, ServiceEvent, ServiceInventoryRequest, ServiceLogSnapshotRequest,
    ServiceLogStreamRequest, ServiceUpdate, SessionControlAction, SessionControlRequest,
    ShellEvent, SmartControlRequest, SmartEvent, SmartObservationRequest, SmartTrackingEndReason,
    SourceOutcome, SourceStatus, StartupControlRequest, StartupEvidenceRequest,
    StartupInventoryRequest, StorageHealthEvent, StorageHealthRequest, SubmissionErrorKind,
    SystemTelemetryRevision, UrlOpenRequest,
};
use taskmanager_core::{
    ContainerRollup, CpuMetrics, CpuTelemetryObservation, DesktopAppearance, DeviceGeneration,
    DeviceId, DeviceState, DeviceStatus, DirectoryScanControl, DirectoryScanSpec,
    DirectoryScanStatus, DirectoryScanTotals, DirectoryUsageSnapshot, FilesystemHealthSnapshot,
    FrozenProcessIdentity, GpuEngineMetric, GpuEngineRowsSnapshot, GpuTelemetryObservation,
    HardwareInfo, HostRuntimeFacts, HostRuntimeObservation, MemoryMetrics,
    MemoryTelemetryObservation, NetworkTelemetryObservation, NpuInventorySnapshot,
    PowerSupplySnapshot, ProcessGpuSnapshot, ProcessIdentity, ProcessInsightSnapshot,
    ProcessIsolation, ProcessItem, ProcessNetworkSnapshot, ProcessOpenFiles,
    ProcessResourceSnapshot, ProcessThreads, ResourceGroupLimitRequest, SensorCenterSnapshot,
    ServiceAction, ServiceDeps, ServiceId, ServiceItem, ServiceLogErrorKind, ServiceLogLevelFilter,
    ServiceLogQuery, ServiceLogState, ServiceLogStreamState, ServiceLogTimeFilter,
    ServiceRelationKind, SessionId, SessionItem, SmartSelfTestIntent, SmartSelfTestReport,
    StartupBootEvidenceSnapshot, StartupEntry, StorageDeviceTarget, StorageTelemetryObservation,
};
use taskmanager_platform_linux::{
    EnvironmentProviders, IntegrationProviders, LinuxPlatformRuntime, LinuxProviderRegistry,
    PowerProviders, ProcessControlProviders, ProcessObservationProviders, ProcessProviders,
    SensorProviders, ServiceProviders, StorageProviders, SystemAuxiliaryProviders,
    SystemObservationProviders, SystemProviders,
};
use taskmanager_platform_provider::{
    CommandLaunchProvider, ContainerRollupProvider, CpuTelemetryProvider,
    DesktopAppearanceProvider, DirectoryUsageProvider, FilesystemHealthProvider,
    GpuEngineRowsProvider, GpuTelemetryProvider, HardwareInventoryProvider, HostTelemetryProvider,
    MemoryTelemetryProvider, NetworkTelemetryProvider, NpuInventoryProvider, PowerSupplyProvider,
    ProcessAffinityControlProvider, ProcessAffinityProvider, ProcessControlProvider,
    ProcessGpuProvider, ProcessIsolationProvider, ProcessListProvider,
    ProcessNetworkEscalationProvider, ProcessNetworkProvider, ProcessOpenFilesProvider,
    ProcessResourceControlProvider, ProcessResourcesProvider, ProcessThreadsProvider,
    ResourceRevealProvider, SensorProvider, ServiceControlProvider, ServiceDependenciesProvider,
    ServiceInventoryProvider, ServiceLogSnapshotProvider, ServiceLogStreamProvider,
    SessionControlProvider, SessionInventoryProvider, SmartSelfTestControlProvider,
    SmartSelfTestObservationProvider, StartupControlProvider, StartupEvidenceProvider,
    StartupInventoryProvider, StorageTelemetryProvider, UrlOpenProvider,
};

#[path = "ports_contract/fixture.rs"]
mod fixture;
use fixture::*;
#[path = "ports_contract/process_affinity_control.rs"]
mod process_affinity_control;
#[path = "ports_contract/process_suspend_resume.rs"]
mod process_suspend_resume;
#[path = "ports_contract/provider_registration.rs"]
mod provider_registration;
use provider_registration::{
    fixture_environment_provider, fixture_process_provider, fixture_service_provider,
    fixture_system_provider,
};
#[path = "ports_contract/service_inventory.rs"]
mod service_inventory;
#[path = "ports_contract/smart_tracking.rs"]
mod smart_tracking;
#[path = "ports_contract/startup_session_control.rs"]
mod startup_session_control;
fn spawn_complete(providers: LinuxProviderRegistry) -> PlatformHandle {
    LinuxPlatformRuntime::spawn_with_providers(providers)
        .expect("fixture registry binds every standard capability")
}

fn assert_capability_degraded(
    handle: &PlatformHandle,
    capability: &CapabilityId,
    failure: FailureKind,
) {
    assert_eq!(
        handle
            .capabilities()
            .snapshot()
            .get(capability)
            .map(|descriptor| descriptor.status),
        Some(CapabilityStatus::Degraded(failure))
    );
}

#[test]
fn desktop_appearance_uses_a_typed_independent_observation_facet() {
    let handle = spawn_complete(fake_registry(FakeProvider::default()));
    let mut ids = RequestIdGenerator::default();
    let request_id = ids.next_id();

    handle
        .desktop_appearance()
        .expect("desktop appearance facet")
        .try_submit(RequestEnvelope {
            id: request_id,
            capability: CapabilityId::DESKTOP_APPEARANCE,
            submitted_at_ms: 1,
            payload: DesktopAppearanceRequest::Observe,
        })
        .expect("bounded appearance request accepted");

    let event = wait_event(&handle);
    assert_eq!(event.request_id, request_id);
    assert_eq!(event.capability, CapabilityId::DESKTOP_APPEARANCE);
    assert_eq!(
        event.provider,
        Some(ProviderId::borrowed(
            "fixture.integration.desktop-appearance"
        )),
        "the event must retain the identity registered with its provider object"
    );
    assert_eq!(
        handle
            .capabilities()
            .snapshot()
            .get(&CapabilityId::DESKTOP_APPEARANCE)
            .map(|descriptor| descriptor.providers.clone()),
        Some(vec![ProviderId::borrowed(
            "fixture.integration.desktop-appearance"
        )]),
        "catalog attribution and execution must derive from the same registration"
    );
    let PlatformEvent::DesktopAppearance(DesktopAppearanceEvent::Snapshot(snapshot)) =
        event.outcome.expect("appearance observation")
    else {
        panic!("expected typed desktop appearance event");
    };
    assert_eq!(snapshot.value, DesktopAppearance::default());
}

#[test]
fn typed_observation_lanes_publish_snapshot_health_instead_of_outer_success() {
    let handle = spawn_complete(fake_registry(FakeProvider {
        observation_source_failure: Some(FailureKind::TimedOut),
        process_telemetry_failure: Some(FailureKind::PermissionDenied),
        ..FakeProvider::default()
    }));
    let mut ids = RequestIdGenerator::default();

    let telemetry_id = ids.next_id();
    let revision = SystemTelemetryRevision::new(1);
    handle
        .cpu_telemetry()
        .expect("CPU telemetry facet")
        .try_submit(RequestEnvelope {
            id: telemetry_id,
            capability: CapabilityId::TELEMETRY_CPU,
            submitted_at_ms: 1,
            payload: CpuTelemetryRequest { revision },
        })
        .expect("CPU telemetry refresh accepted");
    let telemetry_event = wait_event(&handle);
    assert_eq!(telemetry_event.request_id, telemetry_id);
    assert_eq!(
        telemetry_event.provider,
        Some(fixture_system_provider(&telemetry_event.capability))
    );
    assert_capability_degraded(&handle, &CapabilityId::TELEMETRY_CPU, FailureKind::TimedOut);

    let hardware_id = ids.next_id();
    handle
        .hardware_inventory()
        .expect("hardware inventory facet")
        .try_submit(RequestEnvelope {
            id: hardware_id,
            capability: CapabilityId::HARDWARE_INVENTORY,
            submitted_at_ms: 2,
            payload: HardwareInventoryRequest::Refresh,
        })
        .expect("hardware refresh accepted");
    let hardware_event = wait_event(&handle);
    assert_eq!(hardware_event.request_id, hardware_id);
    assert_eq!(
        hardware_event.provider,
        Some(fixture_system_provider(&hardware_event.capability))
    );
    assert_capability_degraded(
        &handle,
        &CapabilityId::HARDWARE_INVENTORY,
        FailureKind::TimedOut,
    );

    let service_id = ids.next_id();
    handle
        .service_inventory()
        .expect("service inventory facet")
        .try_submit(RequestEnvelope {
            id: service_id,
            capability: CapabilityId::SERVICES,
            submitted_at_ms: 3,
            payload: ServiceInventoryRequest::Refresh,
        })
        .expect("service refresh accepted");
    assert_eq!(wait_event(&handle).request_id, service_id);
    assert_capability_degraded(&handle, &CapabilityId::SERVICES, FailureKind::TimedOut);

    let startup_id = ids.next_id();
    handle
        .startup_inventory()
        .expect("startup inventory facet")
        .try_submit(RequestEnvelope {
            id: startup_id,
            capability: CapabilityId::STARTUP,
            submitted_at_ms: 4,
            payload: StartupInventoryRequest::Refresh,
        })
        .expect("startup refresh accepted");
    let startup_event = wait_event(&handle);
    assert_eq!(startup_event.request_id, startup_id);
    assert_eq!(
        startup_event.provider,
        Some(fixture_environment_provider(&startup_event.capability))
    );
    assert_capability_degraded(&handle, &CapabilityId::STARTUP, FailureKind::TimedOut);

    let process_list_id = ids.next_id();
    handle
        .process_list()
        .expect("process list facet")
        .try_submit(RequestEnvelope {
            id: process_list_id,
            capability: CapabilityId::PROCESS_LIST,
            submitted_at_ms: 5,
            payload: ProcessListRequest::Refresh,
        })
        .expect("process list refresh accepted");
    let process_list_event = wait_event(&handle);
    assert_eq!(process_list_event.request_id, process_list_id);
    assert_eq!(
        process_list_event.provider,
        Some(fixture_process_provider(&process_list_event.capability))
    );
    assert_capability_degraded(&handle, &CapabilityId::PROCESS_LIST, FailureKind::TimedOut);

    let process_id = ids.next_id();
    handle
        .process_gpu()
        .expect("process GPU facet")
        .try_submit(RequestEnvelope {
            id: process_id,
            capability: CapabilityId::PROCESS_INSIGHTS_GPU,
            submitted_at_ms: 6,
            payload: ProcessGpuRequest {
                target: frozen_process(42),
                revision: ProcessInsightsRevision::new(1),
            },
        })
        .expect("process insights accepted");
    let process_gpu_event = wait_event(&handle);
    assert_eq!(process_gpu_event.request_id, process_id);
    assert_eq!(
        process_gpu_event.provider,
        Some(fixture_process_provider(&process_gpu_event.capability))
    );
    assert_eq!(
        handle
            .capabilities()
            .snapshot()
            .get(&CapabilityId::PROCESS_INSIGHTS_GPU)
            .map(|descriptor| descriptor.status),
        Some(CapabilityStatus::PermissionRequired),
        "a wholly permission-gated GPU facet is unavailable, not aggregate-degraded"
    );
}

#[test]
fn slow_command_launch_does_not_block_url_open() {
    let handle = spawn_complete(fake_registry(FakeProvider {
        delay: Duration::from_millis(200),
        ..FakeProvider::default()
    }));
    let mut ids = RequestIdGenerator::default();

    handle
        .command_launch()
        .expect("command launch facet")
        .try_submit(RequestEnvelope {
            id: ids.next_id(),
            capability: CapabilityId::COMMAND_LAUNCH,
            submitted_at_ms: 1,
            payload: CommandLaunchRequest {
                command: "slow".into(),
            },
        })
        .expect("command launch accepted");
    let url_request = ids.next_id();
    handle
        .url_open()
        .expect("URL open facet")
        .try_submit(RequestEnvelope {
            id: url_request,
            capability: CapabilityId::URL_OPEN,
            submitted_at_ms: 2,
            payload: UrlOpenRequest {
                url: "https://example.invalid".into(),
            },
        })
        .expect("URL open accepted");

    let event = wait_event(&handle);
    assert_eq!(event.request_id, url_request);
    assert_eq!(event.capability, CapabilityId::URL_OPEN);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::Shell(ShellEvent::TargetOpened))
    ));
}

#[test]
fn resource_reveal_preserves_frozen_process_identity() {
    let provider = FakeProvider::default();
    let revealed = provider.revealed.clone();
    let handle = spawn_complete(fake_registry(provider));
    let mut ids = RequestIdGenerator::default();
    let request_id = ids.next_id();
    let target = frozen_process(42);

    handle
        .resource_reveal()
        .expect("resource reveal facet")
        .try_submit(RequestEnvelope {
            id: request_id,
            capability: CapabilityId::RESOURCE_REVEAL,
            submitted_at_ms: 1,
            payload: ResourceRevealRequest {
                target: target.clone(),
                cached_executable: Some("/fixture/bin/process".into()),
            },
        })
        .expect("resource reveal accepted");

    let event = wait_event(&handle);
    assert_eq!(event.request_id, request_id);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::Shell(ShellEvent::TargetOpened))
    ));
    assert_eq!(
        revealed.lock().expect("revealed targets").as_slice(),
        [target],
        "the resource-reveal worker must receive the frozen identity unchanged"
    );
}

#[test]
fn process_request_returns_before_blocking_provider_finishes() {
    let handle = spawn_complete(fake_registry(FakeProvider {
        delay: Duration::from_millis(150),
        ..Default::default()
    }));
    let mut ids = RequestIdGenerator::default();

    let started = Instant::now();
    let _ = submit_process_list(
        &handle,
        &mut ids,
        CapabilityId::PROCESS_LIST,
        ProcessListRequest::Refresh,
    );
    assert!(
        started.elapsed() < Duration::from_millis(50),
        "request port blocked for {:?}",
        started.elapsed()
    );
    assert!(matches!(
        wait_event(&handle).outcome,
        Ok(PlatformEvent::Processes(ProcessEvent::Snapshot(ref processes)))
            if processes.first().is_some_and(|process| process.pid == 42)
    ));
}

#[test]
fn slow_process_observation_cannot_block_process_control_lane() {
    let process_refresh_started = Arc::new(AtomicBool::new(false));
    let ended = Arc::new(Mutex::new(Vec::new()));
    let handle = spawn_complete(fake_registry(FakeProvider {
        delay: Duration::from_millis(150),
        process_refresh_started: process_refresh_started.clone(),
        ended: ended.clone(),
        ..Default::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let _ = submit_process_list(
        &handle,
        &mut ids,
        CapabilityId::PROCESS_LIST,
        ProcessListRequest::Refresh,
    );
    for _ in 0..100 {
        if process_refresh_started.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(process_refresh_started.load(Ordering::Acquire));

    let target = FrozenProcessIdentity::from_authoritative_parts(77, "control-lane", 500, 5_000)
        .expect("fixture identity");
    let started = Instant::now();
    let control_id = submit_process_control(
        &handle,
        &mut ids,
        CapabilityId::PROCESS_CONTROL,
        ProcessControlRequest::EndTask(target.clone()),
    );
    let event = wait_event(&handle);
    assert_eq!(event.request_id, control_id);
    assert!(started.elapsed() < Duration::from_millis(100));
    assert_eq!(
        ended.lock().expect("ended identities").as_slice(),
        &[target]
    );
}

#[test]
fn slow_process_list_cannot_block_process_insights_lane() {
    let process_refresh_started = Arc::new(AtomicBool::new(false));
    let handle = spawn_complete(fake_registry(FakeProvider {
        delay: Duration::from_millis(150),
        process_refresh_started: process_refresh_started.clone(),
        ..Default::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let list_id = submit_process_list(
        &handle,
        &mut ids,
        CapabilityId::PROCESS_LIST,
        ProcessListRequest::Refresh,
    );
    for _ in 0..100 {
        if process_refresh_started.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(process_refresh_started.load(Ordering::Acquire));

    let insights_id = ids.next_id();
    handle
        .process_network()
        .expect("process network facet")
        .try_submit(RequestEnvelope {
            id: insights_id,
            capability: CapabilityId::PROCESS_INSIGHTS_NETWORK,
            submitted_at_ms: 1,
            payload: ProcessNetworkRequest {
                target: frozen_process(42),
                revision: ProcessInsightsRevision::new(1),
            },
        })
        .expect("process insights request accepted");

    assert_eq!(
        wait_event(&handle).request_id,
        insights_id,
        "process insights must not queue behind a full process scan"
    );
    assert_eq!(wait_event(&handle).request_id, list_id);
}

#[test]
fn slow_process_gpu_cannot_block_network_or_affinity_lanes() {
    let handle = spawn_complete(fake_registry(FakeProvider {
        process_gpu_delay: Duration::from_millis(150),
        ..FakeProvider::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let telemetry_id = ids.next_id();
    handle
        .process_gpu()
        .expect("process GPU facet")
        .try_submit(RequestEnvelope {
            id: telemetry_id,
            capability: CapabilityId::PROCESS_INSIGHTS_GPU,
            submitted_at_ms: 1,
            payload: ProcessGpuRequest {
                target: frozen_process(42),
                revision: ProcessInsightsRevision::new(1),
            },
        })
        .expect("process GPU request accepted");

    let network_id = ids.next_id();
    handle
        .process_network()
        .expect("process network facet")
        .try_submit(RequestEnvelope {
            id: network_id,
            capability: CapabilityId::PROCESS_INSIGHTS_NETWORK,
            submitted_at_ms: 2,
            payload: ProcessNetworkRequest {
                target: frozen_process(42),
                revision: ProcessInsightsRevision::new(1),
            },
        })
        .expect("process network request accepted");

    let affinity_id = ids.next_id();
    handle
        .process_affinity()
        .expect("process affinity facet")
        .try_submit(RequestEnvelope {
            id: affinity_id,
            capability: CapabilityId::PROCESS_AFFINITY,
            submitted_at_ms: 2,
            payload: ProcessAffinityRequest {
                target: FrozenProcessIdentity::from_authoritative_parts(
                    42, "fixture", 7_500, 9_000,
                )
                .expect("fixture identity"),
            },
        })
        .expect("process affinity accepted");

    let first = wait_event(&handle);
    let second = wait_event(&handle);
    let quick_ids = [first.request_id, second.request_id];
    assert!(quick_ids.contains(&network_id));
    assert!(quick_ids.contains(&affinity_id));
    assert_eq!(wait_event(&handle).request_id, telemetry_id);
}

#[test]
fn slow_service_inventory_cannot_block_service_control_lane() {
    let handle = spawn_complete(fake_registry(FakeProvider {
        delay: Duration::from_millis(150),
        ..Default::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let inventory_id = ids.next_id();
    handle
        .service_inventory()
        .expect("service inventory facet")
        .try_submit(RequestEnvelope {
            id: inventory_id,
            capability: CapabilityId::SERVICES,
            submitted_at_ms: 1,
            payload: ServiceInventoryRequest::Refresh,
        })
        .expect("service inventory request accepted");

    let control_id = ids.next_id();
    let mut control_generations = LatestControlRequest::default();
    handle
        .service_control()
        .expect("service control facet")
        .try_submit(RequestEnvelope {
            id: control_id,
            capability: CapabilityId::SERVICE_CONTROL,
            submitted_at_ms: 1,
            payload: ServiceControlRequest {
                request_id: control_generations.begin(),
                service_id: "fixture.service".into(),
                action: ServiceAction::Restart,
            },
        })
        .expect("service control request accepted");

    let control = wait_event(&handle);
    assert_eq!(
        control.request_id, control_id,
        "service control must not queue behind service inventory"
    );
    assert_eq!(control.capability, CapabilityId::SERVICE_CONTROL);
    assert_eq!(
        control.provider,
        Some(fixture_service_provider(&control.capability))
    );
    let inventory = wait_event(&handle);
    assert_eq!(inventory.request_id, inventory_id);
    assert_eq!(
        inventory.provider,
        Some(fixture_service_provider(&inventory.capability))
    );
}

#[test]
fn slow_service_inventory_cannot_block_dependency_query_lane() {
    let handle = spawn_complete(fake_registry(FakeProvider {
        delay: Duration::from_millis(150),
        ..Default::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let inventory_id = ids.next_id();
    handle
        .service_inventory()
        .expect("service inventory facet")
        .try_submit(RequestEnvelope {
            id: inventory_id,
            capability: CapabilityId::SERVICES,
            submitted_at_ms: 1,
            payload: ServiceInventoryRequest::Refresh,
        })
        .expect("service inventory request accepted");

    let dependencies_id = ids.next_id();
    handle
        .service_dependencies()
        .expect("service dependencies facet")
        .try_submit(RequestEnvelope {
            id: dependencies_id,
            capability: CapabilityId::SERVICE_DEPENDENCIES,
            submitted_at_ms: 2,
            payload: ServiceDependenciesRequest {
                service_id: "fixture.service".into(),
            },
        })
        .expect("service dependencies request accepted");

    let dependencies = wait_event(&handle);
    assert_eq!(dependencies.request_id, dependencies_id);
    assert_eq!(dependencies.capability, CapabilityId::SERVICE_DEPENDENCIES);
    assert_eq!(
        dependencies.provider,
        Some(fixture_service_provider(&dependencies.capability))
    );
    let inventory = wait_event(&handle);
    assert_eq!(inventory.request_id, inventory_id);
    assert_eq!(
        inventory.provider,
        Some(fixture_service_provider(&inventory.capability))
    );
}

#[test]
fn health_capabilities_are_independent_correlated_facets() {
    let smart_starts = Arc::new(Mutex::new(Vec::new()));
    let handle = spawn_complete(fake_registry(FakeProvider {
        delay: Duration::from_millis(150),
        smart_starts: smart_starts.clone(),
        ..Default::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let storage_id = ids.next_id();
    handle
        .storage_health()
        .expect("storage health facet")
        .try_submit(RequestEnvelope {
            id: storage_id,
            capability: CapabilityId::STORAGE_HEALTH,
            submitted_at_ms: 1,
            payload: StorageHealthRequest::Refresh,
        })
        .expect("storage refresh accepted");

    let smart_id = ids.next_id();
    let intent = SmartSelfTestIntent {
        device_id: DeviceId::new("disk:fixture-device"),
        device_generation: DeviceGeneration::INITIAL,
        device_key: "fixture-device".into(),
        display_name: "Fixture disk".into(),
        kind: taskmanager_core::SmartSelfTestKind::Short,
    };
    handle
        .smart_control()
        .expect("SMART control facet")
        .try_submit(RequestEnvelope {
            id: smart_id,
            capability: CapabilityId::SMART_CONTROL,
            submitted_at_ms: 1,
            payload: SmartControlRequest::StartSelfTest(intent.clone()),
        })
        .expect("SMART start accepted");

    let first = wait_event(&handle);
    assert_eq!(
        first.request_id, smart_id,
        "slow filesystem discovery must not block SMART control"
    );
    assert!(matches!(
        first.outcome,
        Ok(PlatformEvent::Smart(SmartEvent::Batch(ref batch)))
            if batch.observations.iter().any(|observation|
                observation.device_key == intent.device_key
                    && observation.device_id == intent.device_id
                    && observation.device_generation == intent.device_generation)
    ));
    assert_eq!(
        smart_starts.lock().expect("SMART starts").as_slice(),
        &[intent]
    );

    let storage = wait_event(&handle);
    assert_eq!(storage.request_id, storage_id);
    assert!(matches!(
        storage.outcome,
        Ok(PlatformEvent::StorageHealth(StorageHealthEvent::Snapshot(
            _
        )))
    ));

    let sensor_id = ids.next_id();
    handle
        .sensors()
        .expect("sensor facet")
        .try_submit(RequestEnvelope {
            id: sensor_id,
            capability: CapabilityId::SENSORS,
            submitted_at_ms: 1,
            payload: SensorRequest::Refresh,
        })
        .expect("sensor refresh accepted");
    let sensor = wait_event(&handle);
    assert_eq!(sensor.request_id, sensor_id);
    assert!(matches!(
        sensor.outcome,
        Ok(PlatformEvent::Sensors(SensorEvent::Snapshot(_)))
    ));

    let power_id = ids.next_id();
    handle
        .power_supplies()
        .expect("power-supply facet")
        .try_submit(RequestEnvelope {
            id: power_id,
            capability: CapabilityId::POWER_SUPPLIES,
            submitted_at_ms: 1,
            payload: PowerSupplyRequest::Refresh,
        })
        .expect("power-supply refresh accepted");
    let power = wait_event(&handle);
    assert_eq!(power.request_id, power_id);
    assert!(matches!(
        power.outcome,
        Ok(PlatformEvent::PowerSupplies(PowerSupplyEvent::Snapshot(_)))
    ));

    let capabilities = handle.capabilities().snapshot();
    for capability in [
        CapabilityId::STORAGE_HEALTH,
        CapabilityId::SENSORS,
        CapabilityId::POWER_SUPPLIES,
        CapabilityId::SMART_CONTROL,
    ] {
        assert_eq!(
            capabilities
                .get(&capability)
                .map(|descriptor| descriptor.status),
            Some(CapabilityStatus::Available)
        );
    }
}

#[test]
fn sensor_enrichment_failure_keeps_event_and_degrades_capability_health() {
    let handle = spawn_complete(fake_registry(FakeProvider {
        sensor_enrichment_error: Some(FailureKind::PermissionDenied),
        ..FakeProvider::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let request_id = ids.next_id();
    handle
        .sensors()
        .expect("sensor facet")
        .try_submit(RequestEnvelope {
            id: request_id,
            capability: CapabilityId::SENSORS,
            submitted_at_ms: 1,
            payload: SensorRequest::Refresh,
        })
        .expect("sensor refresh accepted");

    let event = wait_event(&handle);
    assert_eq!(event.request_id, request_id);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::Sensors(SensorEvent::Snapshot(_)))
    ));
    let descriptor = handle
        .capabilities()
        .snapshot()
        .get(&CapabilityId::SENSORS)
        .cloned()
        .expect("sensor capability");
    assert_eq!(
        descriptor.status,
        CapabilityStatus::Degraded(FailureKind::PermissionDenied)
    );
    assert!(descriptor.last_success_at_ms.is_some());
}

#[test]
fn provider_failure_keeps_capability_and_typed_reason() {
    let handle = spawn_complete(fake_registry(FakeProvider {
        service_error: Some(ProviderFailure::PermissionDenied),
        ..Default::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let request_id = ids.next_id();
    handle
        .service_inventory()
        .expect("service inventory facet")
        .try_submit(RequestEnvelope {
            id: request_id,
            capability: CapabilityId::SERVICES,
            submitted_at_ms: 1,
            payload: ServiceInventoryRequest::Refresh,
        })
        .expect("service request accepted");

    let failure = wait_event(&handle)
        .outcome
        .expect_err("provider failure expected");
    assert_eq!(failure.request_id, request_id);
    assert_eq!(failure.capability, CapabilityId::SERVICES);
    assert_eq!(failure.kind, FailureKind::PermissionDenied);
    assert_eq!(
        handle
            .capabilities()
            .snapshot()
            .get(&CapabilityId::SERVICES)
            .map(|descriptor| descriptor.status),
        Some(CapabilityStatus::PermissionRequired)
    );
}

#[test]
fn service_detail_failures_return_typed_domain_events_and_update_capabilities() {
    let handle = spawn_complete(fake_registry(FakeProvider {
        service_operation_error: Some(ProviderFailure::PermissionDenied),
        ..Default::default()
    }));
    let mut ids = RequestIdGenerator::default();
    let dependencies_id = ids.next_id();
    handle
        .service_dependencies()
        .expect("service dependencies facet")
        .try_submit(RequestEnvelope {
            id: dependencies_id,
            capability: CapabilityId::SERVICE_DEPENDENCIES,
            submitted_at_ms: 1,
            payload: ServiceDependenciesRequest {
                service_id: "denied.service".into(),
            },
        })
        .expect("dependencies request accepted");
    let control_id = ids.next_id();
    let mut control_generations = LatestControlRequest::default();
    handle
        .service_control()
        .expect("service control facet")
        .try_submit(RequestEnvelope {
            id: control_id,
            capability: CapabilityId::SERVICE_CONTROL,
            submitted_at_ms: 2,
            payload: ServiceControlRequest {
                request_id: control_generations.begin(),
                service_id: "denied.service".into(),
                action: ServiceAction::Restart,
            },
        })
        .expect("control request accepted");
    let logs_id = ids.next_id();
    handle
        .service_log_snapshot()
        .expect("service log snapshot facet")
        .try_submit(RequestEnvelope {
            id: logs_id,
            capability: CapabilityId::SERVICE_LOGS,
            submitted_at_ms: 3,
            payload: ServiceLogSnapshotRequest {
                service_id: "denied.service".into(),
            },
        })
        .expect("log request accepted");
    let stream_id = ids.next_id();
    handle
        .service_log_stream()
        .expect("service log stream facet")
        .try_submit(RequestEnvelope {
            id: stream_id,
            capability: CapabilityId::SERVICE_LOG_STREAM,
            submitted_at_ms: 4,
            payload: ServiceLogStreamRequest {
                query: ServiceLogQuery {
                    service_id: "denied.service".into(),
                    level: ServiceLogLevelFilter::All,
                    time: ServiceLogTimeFilter::All,
                    after_cursor: None,
                },
            },
        })
        .expect("log stream request accepted");

    let mut dependencies_seen = false;
    let mut control_seen = false;
    let mut logs_seen = false;
    let mut stream_seen = false;
    for _ in 0..4 {
        let event = wait_event(&handle);
        assert_eq!(
            event.provider,
            Some(fixture_service_provider(&event.capability))
        );
        match event.outcome {
            Ok(PlatformEvent::Services(ServiceEvent::Update(
                ServiceUpdate::DependenciesUnavailable { error, .. },
            ))) => {
                assert_eq!(event.request_id, dependencies_id);
                assert_eq!(error, FailureKind::PermissionDenied);
                dependencies_seen = true;
            }
            Ok(PlatformEvent::Services(ServiceEvent::Update(ServiceUpdate::Action(outcome)))) => {
                assert_eq!(event.request_id, control_id);
                assert_eq!(outcome.result, Err(FailureKind::PermissionDenied));
                control_seen = true;
            }
            Ok(PlatformEvent::Services(ServiceEvent::Update(ServiceUpdate::Logs(snapshot)))) => {
                assert_eq!(event.request_id, logs_id);
                assert!(matches!(
                    snapshot.state,
                    ServiceLogState::Unavailable(ref failure)
                        if failure.kind == ServiceLogErrorKind::PermissionDenied
                ));
                logs_seen = true;
            }
            Ok(PlatformEvent::Services(ServiceEvent::Update(ServiceUpdate::LogStream {
                snapshot,
                ..
            }))) => {
                assert_eq!(event.request_id, stream_id);
                assert!(matches!(
                    snapshot.state,
                    ServiceLogStreamState::Unavailable(ref failure)
                        if failure.kind == ServiceLogErrorKind::PermissionDenied
                ));
                stream_seen = true;
            }
            other => panic!("unexpected service detail outcome: {other:?}"),
        }
    }
    assert!(dependencies_seen && control_seen && logs_seen && stream_seen);

    let capabilities = handle.capabilities().snapshot();
    for capability in [
        CapabilityId::SERVICE_DEPENDENCIES,
        CapabilityId::SERVICE_CONTROL,
        CapabilityId::SERVICE_LOGS,
        CapabilityId::SERVICE_LOG_STREAM,
    ] {
        assert_eq!(
            capabilities
                .get(&capability)
                .map(|descriptor| descriptor.status),
            Some(CapabilityStatus::PermissionRequired),
            "{capability} terminal must publish permission health atomically"
        );
    }
}

#[test]
fn confirmed_process_identity_reaches_worker_unchanged() {
    let ended = Arc::new(Mutex::new(Vec::new()));
    let handle = spawn_complete(fake_registry(FakeProvider {
        ended: ended.clone(),
        ..Default::default()
    }));
    let target = FrozenProcessIdentity::from_authoritative_parts(77, "confirmed", 500, 5_000)
        .expect("fixture identity");
    let mut ids = RequestIdGenerator::default();
    let _ = submit_process_control(
        &handle,
        &mut ids,
        CapabilityId::PROCESS_CONTROL,
        ProcessControlRequest::EndTask(target.clone()),
    );
    assert!(matches!(
        wait_event(&handle).outcome,
        Ok(PlatformEvent::Processes(
            ProcessEvent::EndTaskCompleted(ref completed)
        )) if completed == &target
    ));
    assert_eq!(
        ended.lock().expect("ended identities").as_slice(),
        &[target]
    );
}

#[test]
fn procfs_identity_remains_distinct_from_confirmed_ui_identity() {
    let telemetry = ProcessIdentity {
        pid: 42,
        start_token: 7_500,
    };
    let confirmed = FrozenProcessIdentity::from_authoritative_parts(42, "worker", 75, 7_500)
        .expect("fixture identity");
    assert_eq!(telemetry.pid, confirmed.pid);
    assert_eq!(
        Some(telemetry.start_token),
        confirmed.authoritative_start_token()
    );
    assert_ne!(telemetry.start_token, confirmed.start_time_secs);
}
