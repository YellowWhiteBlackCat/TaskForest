use std::thread;
use std::time::Duration;

use taskmanager_application::{
    ContainerRollupEvent, ContainerRollupRequest, CpuTelemetryRequest, MemoryTelemetryRequest,
    MsrReadoutEvent, MsrReadoutRequest, PlatformEvent, RaplPowerEvent, RaplPowerRequest,
    SmbiosMemoryEvent, SmbiosMemoryRequest, StorageTelemetryRequest, SystemTelemetryDomainEvent,
    SystemTelemetryRevision,
};
use taskmanager_core::core::identity::ProviderId;
use taskmanager_core::{
    ContainerRollup, ContainerSummary, FailureKind, IsolationKind, ScalarObservation,
};
use taskmanager_platform_contract::{CapabilityId, RequestEnvelope, RequestId};

use super::*;
use crate::{ProviderBinding, RuntimeConfig, RuntimeProviderBindings};

fn fixed_clock() -> u64 {
    17
}

fn system_bindings() -> RuntimeProviderBindings {
    let mut bindings = RuntimeProviderBindings::default();
    bindings.system.host = ProviderBinding::present(ProviderId::borrowed("fixture.system.host"));
    bindings.system.cpu = ProviderBinding::present(ProviderId::borrowed("fixture.system.cpu"));
    bindings.system.memory =
        ProviderBinding::present(ProviderId::borrowed("fixture.system.memory"));
    bindings.system.storage =
        ProviderBinding::present(ProviderId::borrowed("fixture.system.storage"));
    bindings.system.network =
        ProviderBinding::present(ProviderId::borrowed("fixture.system.network"));
    bindings.system.gpu = ProviderBinding::present(ProviderId::borrowed("fixture.system.gpu"));
    bindings.system.hardware_inventory =
        ProviderBinding::present(ProviderId::borrowed("fixture.system.hardware-inventory"));
    bindings.system.containers =
        ProviderBinding::present(ProviderId::borrowed("fixture.system.containers"));
    bindings
}

fn registered_system_provider(capability: &CapabilityId) -> ProviderId {
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
    } else if capability == &CapabilityId::CONTAINERS {
        ProviderId::borrowed("fixture.system.containers")
    } else {
        panic!("unexpected system capability {capability}");
    }
}

#[test]
fn system_catalog_keeps_eight_distinct_registered_provider_identities() {
    let runtime = crate::ChannelRuntime::new(system_bindings(), RuntimeConfig::new(fixed_clock));
    let capabilities = runtime.handle.capabilities().snapshot();

    for capability in [
        CapabilityId::TELEMETRY_HOST,
        CapabilityId::TELEMETRY_CPU,
        CapabilityId::TELEMETRY_MEMORY,
        CapabilityId::TELEMETRY_STORAGE,
        CapabilityId::TELEMETRY_NETWORK,
        CapabilityId::TELEMETRY_GPU,
        CapabilityId::HARDWARE_INVENTORY,
        CapabilityId::CONTAINERS,
    ] {
        assert_eq!(
            capabilities
                .get(&capability)
                .map(|descriptor| descriptor.providers.clone()),
            Some(vec![registered_system_provider(&capability)])
        );
    }
}

#[test]
fn pending_system_group_promotes_atomically_and_reports_one_missing_lane() {
    let complete = crate::ChannelRuntime::new(system_bindings(), RuntimeConfig::new(fixed_clock));
    assert_eq!(complete.lanes.system.missing_capabilities().count(), 0);
    assert!(complete.lanes.system.try_complete().is_some());

    let mut incomplete_bindings = system_bindings();
    incomplete_bindings.system.hardware_inventory = ProviderBinding::absent();
    let incomplete =
        crate::ChannelRuntime::new(incomplete_bindings, RuntimeConfig::new(fixed_clock));
    assert_eq!(
        incomplete
            .lanes
            .system
            .missing_capabilities()
            .collect::<Vec<_>>(),
        [CapabilityId::HARDWARE_INVENTORY]
    );
    assert!(incomplete.lanes.system.try_complete().is_none());
}

#[test]
fn slow_storage_lane_does_not_block_cpu_completion() {
    let runtime = crate::ChannelRuntime::new(system_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
        ..
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_system_lanes(
        &workers,
        lanes.system.try_complete().expect("complete system lanes"),
        SystemExecutors::new(
            SystemObservationExecutors::new(
                |_observed_at_ms| Err(ProviderFailure::Unsupported),
                |observed_at_ms| {
                    assert_eq!(observed_at_ms, fixed_clock());
                    Ok(taskmanager_core::CpuTelemetryObservation::unavailable(
                        FailureKind::Unsupported,
                        Vec::new(),
                    ))
                },
                |observed_at_ms| {
                    assert_eq!(observed_at_ms, fixed_clock());
                    Ok(taskmanager_core::MemoryTelemetryObservation::unavailable(
                        FailureKind::Unsupported,
                        Vec::new(),
                    ))
                },
                |_observed_at_ms| {
                    thread::sleep(Duration::from_millis(150));
                    Ok(taskmanager_core::StorageTelemetryObservation::unavailable(
                        FailureKind::Unsupported,
                        Vec::new(),
                        Vec::new(),
                        Default::default(),
                    ))
                },
                |_observed_at_ms| Err(ProviderFailure::Unsupported),
                |_observed_at_ms| Err(ProviderFailure::Unsupported),
                |_now_ms| Err(ProviderFailure::Unsupported),
            ),
            SystemAuxiliaryExecutors::new(|| Err(ProviderFailure::Unsupported)),
        ),
        publisher,
        fixed_clock,
    )
    .expect("system workers start");
    let revision = SystemTelemetryRevision::new(1);
    handle
        .storage_telemetry()
        .expect("storage telemetry port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(1).expect("request id"),
            capability: CapabilityId::TELEMETRY_STORAGE,
            submitted_at_ms: 1,
            payload: StorageTelemetryRequest { revision },
        })
        .expect("storage accepted");
    handle
        .cpu_telemetry()
        .expect("cpu telemetry port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(2).expect("request id"),
            capability: CapabilityId::TELEMETRY_CPU,
            submitted_at_ms: 1,
            payload: CpuTelemetryRequest { revision },
        })
        .expect("cpu accepted");
    handle
        .memory_telemetry()
        .expect("memory telemetry port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(3).expect("request id"),
            capability: CapabilityId::TELEMETRY_MEMORY,
            submitted_at_ms: 1,
            payload: MemoryTelemetryRequest { revision },
        })
        .expect("memory accepted");

    let mut saw_cpu = false;
    let mut saw_memory = false;
    for _ in 0..50 {
        if let Some(event) = handle.events().try_recv().expect("event port") {
            assert_eq!(
                event.provider,
                Some(registered_system_provider(&event.capability))
            );
            match event.outcome {
                Ok(PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Cpu { .. })) => {
                    saw_cpu = true
                }
                Ok(PlatformEvent::SystemTelemetry(SystemTelemetryDomainEvent::Memory {
                    ..
                })) => saw_memory = true,
                other => panic!("slow storage completed before CPU/memory: {other:?}"),
            }
            if saw_cpu && saw_memory {
                return;
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("CPU or memory completion was blocked behind the slow storage provider");
}

/// The container observation lane must drive a real `ContainerRollupProvider`
/// closure through the runtime lane, map the result to
/// `PlatformEvent::Containers(ContainerRollupEvent::Snapshot(..))`, and publish
/// it on the event port. This is the only end-to-end test that exercises the
/// live-polling wire: break the `ContainerExecutor` closure binding or the event
/// mapper in `spawn_system_lanes` and this test fails, where the
/// `apply_platform_event_batch` unit test (which feeds a hand-built batch) would
/// still pass. Uses a stub rollup — no real cgroups/proc.
#[test]
fn container_lane_emits_snapshot_event_carrying_the_provider_rollup() {
    // A stub rollup with one known container summary, distinguishable from the
    // empty-healthy default. The test asserts THIS exact rollup round-trips.
    let stub_rollup = {
        let mut rollup = ContainerRollup::empty_healthy(17);
        rollup.containers.push(ContainerSummary {
            id: "/docker/stub-abc".into(),
            name: "stub-abc".into(),
            runtime: Some(IsolationKind::Docker),
            cgroup_path: "/docker/stub-abc".into(),
            cpu_percentage: ScalarObservation::available(42.0, 17),
            memory_bytes: ScalarObservation::available(1_048_576, 17),
            member_pids: vec![1111, 2222],
        });
        rollup
    };
    let expected = stub_rollup.clone();

    let runtime = crate::ChannelRuntime::new(system_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
        ..
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_system_lanes(
        &workers,
        lanes.system.try_complete().expect("complete system lanes"),
        SystemExecutors::new(
            SystemObservationExecutors::new(
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                move |_now_ms| Ok(stub_rollup.clone()),
            ),
            SystemAuxiliaryExecutors::new(|| Err(ProviderFailure::Unsupported)),
        ),
        publisher,
        fixed_clock,
    )
    .expect("system workers start");

    // Drive a real RefreshRequest::Containers through the typed port — the same
    // path the application client uses. The lane must observe + map + publish.
    let containers_port = handle
        .facets()
        .system()
        .containers()
        .expect("containers port is wired when the binding is present");
    containers_port
        .try_submit(RequestEnvelope {
            id: RequestId::new(7).expect("request id"),
            capability: CapabilityId::CONTAINERS,
            submitted_at_ms: 1,
            payload: ContainerRollupRequest::Refresh,
        })
        .expect("containers refresh accepted by the lane");

    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("event port") {
            assert_eq!(
                event.provider,
                Some(registered_system_provider(&CapabilityId::CONTAINERS))
            );
            match event.outcome {
                Ok(PlatformEvent::Containers(ContainerRollupEvent::Snapshot(boxed))) => {
                    // The stub rollup must arrive unchanged — proving the
                    // executor closure fed the event mapper.
                    assert_eq!(*boxed, expected);
                    assert_eq!(boxed.containers.len(), 1);
                    assert_eq!(boxed.containers[0].id, "/docker/stub-abc");
                    return;
                }
                Ok(other) => panic!("expected a Containers event, got {other:?}"),
                Err(failure) => panic!("container lane reported a failure: {failure:?}"),
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("no ContainerRollup snapshot event arrived from the live observation lane");
}

/// Drive one real `SmbiosMemoryRequest::Refresh` through the typed port: the
/// lane must run the executor closure once and publish exactly one correlated
/// `SmbiosMemoryEvent::Update` carrying the provider's snapshot (the same
/// wire the application client consumes).
#[test]
fn smbios_memory_lane_emits_update_event_for_a_refresh_request() {
    let mut bindings = system_bindings();
    bindings.system.smbios_memory =
        ProviderBinding::present(ProviderId::borrowed("fixture.system.smbios-memory"));
    let runtime = crate::ChannelRuntime::new(bindings, RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
        ..
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_system_lanes(
        &workers,
        lanes.system.try_complete().expect("complete system lanes"),
        SystemExecutors::new(
            SystemObservationExecutors::new(
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
            ),
            SystemAuxiliaryExecutors::new(|| Err(ProviderFailure::Unsupported)).with_smbios_memory(
                || {
                    Ok(taskmanager_core::SmbiosMemorySnapshot::success(
                        4,
                        2,
                        vec![taskmanager_core::SmbiosModuleRow {
                            slot: 1,
                            size_mb: Some(32_768),
                            ..taskmanager_core::SmbiosModuleRow::default()
                        }],
                        None,
                    ))
                },
            ),
        ),
        publisher,
        fixed_clock,
    )
    .expect("system workers start");

    handle
        .facets()
        .system()
        .smbios_memory()
        .expect("smbios port is wired when the binding is present")
        .try_submit(RequestEnvelope {
            id: RequestId::new(8).expect("request id"),
            capability: CapabilityId::TELEMETRY_MEMORY_SMBIOS,
            submitted_at_ms: 1,
            payload: SmbiosMemoryRequest::Refresh,
        })
        .expect("smbios refresh accepted by the lane");

    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("event port") {
            match event.outcome {
                Ok(PlatformEvent::SmbiosMemory(SmbiosMemoryEvent::Update(snapshot))) => {
                    assert!(snapshot.is_success());
                    assert_eq!(snapshot.slots_total, 4);
                    assert_eq!(snapshot.slots_used, 2);
                    assert_eq!(snapshot.modules.len(), 1);
                    return;
                }
                Ok(other) => panic!("expected a SmbiosMemory event, got {other:?}"),
                Err(failure) => panic!("smbios lane reported a failure: {failure:?}"),
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("no SmbiosMemory update event arrived from the live lane");
}

/// Drive one real `RaplPowerRequest::Refresh` through the typed port: the
/// lane must run the executor closure once and publish exactly one correlated
/// `RaplPowerEvent::Update` carrying the provider's snapshot.
#[test]
fn rapl_power_lane_emits_update_event_for_a_refresh_request() {
    let mut bindings = system_bindings();
    bindings.system.rapl_power =
        ProviderBinding::present(ProviderId::borrowed("fixture.system.rapl-power"));
    let runtime = crate::ChannelRuntime::new(bindings, RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
        ..
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_system_lanes(
        &workers,
        lanes.system.try_complete().expect("complete system lanes"),
        SystemExecutors::new(
            SystemObservationExecutors::new(
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
            ),
            SystemAuxiliaryExecutors::new(|| Err(ProviderFailure::Unsupported)).with_rapl_power(
                || {
                    Ok(taskmanager_core::RaplPowerSnapshot::success(
                        250,
                        vec![taskmanager_core::RaplPackageRow {
                            name: "package-1".to_owned(),
                            power_w: 9.5,
                            energy_delta_uj: 2_375_000,
                        }],
                    ))
                },
            ),
        ),
        publisher,
        fixed_clock,
    )
    .expect("system workers start");

    handle
        .facets()
        .system()
        .rapl_power()
        .expect("rapl port is wired when the binding is present")
        .try_submit(RequestEnvelope {
            id: RequestId::new(9).expect("request id"),
            capability: CapabilityId::TELEMETRY_CPU_PACKAGE_POWER,
            submitted_at_ms: 1,
            payload: RaplPowerRequest::Refresh,
        })
        .expect("rapl refresh accepted by the lane");

    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("event port") {
            match event.outcome {
                Ok(PlatformEvent::RaplPower(RaplPowerEvent::Update(snapshot))) => {
                    assert!(snapshot.is_success());
                    assert_eq!(snapshot.sample_ms, 250);
                    assert_eq!(snapshot.packages.len(), 1);
                    assert_eq!(snapshot.packages[0].power_w, 9.5);
                    return;
                }
                Ok(other) => panic!("expected a RaplPower event, got {other:?}"),
                Err(failure) => panic!("rapl lane reported a failure: {failure:?}"),
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("no RaplPower update event arrived from the live lane");
}

/// Drive one real `MsrReadoutRequest::Refresh` through the typed port: the
/// lane must run the executor closure once and publish exactly one correlated
/// `MsrReadoutEvent::Update` carrying the provider's snapshot.
#[test]
fn msr_readout_lane_emits_update_event_for_a_refresh_request() {
    let mut bindings = system_bindings();
    bindings.system.msr_readout =
        ProviderBinding::present(ProviderId::borrowed("fixture.system.msr-readout"));
    let runtime = crate::ChannelRuntime::new(bindings, RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
        ..
    } = runtime;
    let workers = crate::WorkerRuntime::default();
    spawn_system_lanes(
        &workers,
        lanes.system.try_complete().expect("complete system lanes"),
        SystemExecutors::new(
            SystemObservationExecutors::new(
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
                |_| Err(ProviderFailure::Unsupported),
            ),
            SystemAuxiliaryExecutors::new(|| Err(ProviderFailure::Unsupported)).with_msr_readout(
                || {
                    Ok(taskmanager_core::MsrReadoutSnapshot::success(vec![
                        taskmanager_core::MsrPackageReadout {
                            cpu: 1,
                            bclk_mhz: None,
                            temperature_c: Some(54.5),
                            multiplier: Some(42.0),
                            multiplier_min: Some(8.0),
                            multiplier_max: Some(58.0),
                            vcore_v: Some(1.219),
                        },
                    ]))
                },
            ),
        ),
        publisher,
        fixed_clock,
    )
    .expect("system workers start");

    handle
        .facets()
        .system()
        .msr_readout()
        .expect("msr port is wired when the binding is present")
        .try_submit(RequestEnvelope {
            id: RequestId::new(10).expect("request id"),
            capability: CapabilityId::TELEMETRY_CPU_MSR,
            submitted_at_ms: 1,
            payload: MsrReadoutRequest::Refresh,
        })
        .expect("msr refresh accepted by the lane");

    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("event port") {
            match event.outcome {
                Ok(PlatformEvent::MsrReadout(MsrReadoutEvent::Update(snapshot))) => {
                    assert!(snapshot.is_success());
                    assert_eq!(snapshot.packages.len(), 1);
                    assert_eq!(snapshot.packages[0].cpu, 1);
                    assert_eq!(snapshot.packages[0].temperature_c, Some(54.5));
                    assert_eq!(snapshot.packages[0].vcore_v, Some(1.219));
                    return;
                }
                Ok(other) => panic!("expected a MsrReadout event, got {other:?}"),
                Err(failure) => panic!("msr lane reported a failure: {failure:?}"),
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("no MsrReadout update event arrived from the live lane");
}
