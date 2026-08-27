use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use taskmanager_application::{
    CapabilityId, PartialSourceSnapshot, PlatformEvent, ProcessAffinityEvent,
    ProcessAffinityRequest, ProcessControlRequest, ProcessEnvironmentRequest, ProcessEvent,
    ProcessInsightFacetEvent, ProcessInsightsRevision, ProcessListRequest, ProcessNetworkRequest,
    ProcessOpenFilesRequest, ProcessThreadsRequest, ProviderId, RequestEnvelope, RequestId,
    SourceOutcome, SourceStatus,
};
use taskmanager_core::{
    DeviceState, FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessBatchResult,
    ProcessBatchTargetResult, ProcessEnvironment, ProcessGpuSnapshot, ProcessIdentity,
    ProcessInsightSnapshot, ProcessIsolation, ProcessItem, ProcessNetworkSnapshot,
    ProcessOpenFiles, ProcessResourceSnapshot, ProcessThreads, ResourceGroupLimitRequest,
};

use super::*;
use crate::{ProcessProviderBindings, ProviderBinding, RuntimeConfig, RuntimeProviderBindings};

const CLOCK_MS: u64 = 4_242;

fn fixed_clock() -> u64 {
    CLOCK_MS
}

#[derive(Clone, Default)]
struct FixtureState {
    batches: Arc<Mutex<Vec<ProcessBatchIntent>>>,
    list_times: Arc<Mutex<Vec<u64>>>,
    telemetry_times: Arc<Mutex<Vec<u64>>>,
}

fn process_bindings() -> RuntimeProviderBindings {
    RuntimeProviderBindings {
        process: ProcessProviderBindings {
            list: ProviderBinding::present(ProviderId::borrowed("fixture.process.list")),
            control: ProviderBinding::present(ProviderId::borrowed("fixture.process.control")),
            network: ProviderBinding::present(ProviderId::borrowed("fixture.process.network")),
            gpu: ProviderBinding::present(ProviderId::borrowed("fixture.process.gpu")),
            resources: ProviderBinding::present(ProviderId::borrowed("fixture.process.resources")),
            isolation: ProviderBinding::present(ProviderId::borrowed("fixture.process.isolation")),
            threads: ProviderBinding::present(ProviderId::borrowed("fixture.process.threads")),
            open_files: ProviderBinding::present(ProviderId::borrowed(
                "fixture.process.open_files",
            )),
            environment: ProviderBinding::present(ProviderId::borrowed(
                "fixture.process.environment",
            )),
            affinity: ProviderBinding::present(ProviderId::borrowed("fixture.process.affinity")),
            affinity_control: ProviderBinding::present(ProviderId::borrowed(
                "fixture.process.affinity-control",
            )),
            resource_control: ProviderBinding::present(ProviderId::borrowed(
                "fixture.process.resource-control",
            )),
            network_escalation: ProviderBinding::present(ProviderId::borrowed(
                "fixture.process.network-escalation",
            )),
        },
        ..RuntimeProviderBindings::default()
    }
}

fn registered_process_provider(capability: &CapabilityId) -> ProviderId {
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
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_OPEN_FILES {
        ProviderId::borrowed("fixture.process.open_files")
    } else if capability == &CapabilityId::PROCESS_INSIGHTS_ENVIRONMENT {
        ProviderId::borrowed("fixture.process.environment")
    } else if capability == &CapabilityId::PROCESS_AFFINITY {
        ProviderId::borrowed("fixture.process.affinity")
    } else if capability == &CapabilityId::PROCESS_AFFINITY_CONTROL {
        ProviderId::borrowed("fixture.process.affinity-control")
    } else if capability == &CapabilityId::PROCESS_RESOURCE_CONTROL {
        ProviderId::borrowed("fixture.process.resource-control")
    } else {
        panic!("unexpected process capability {capability}");
    }
}

fn assert_registered_process_provider(
    event: &taskmanager_application::EventEnvelope<PlatformEvent>,
) {
    assert_eq!(
        event.provider,
        Some(registered_process_provider(&event.capability))
    );
}

#[test]
fn process_catalog_keeps_distinct_registered_provider_identities() {
    let runtime = crate::ChannelRuntime::new(process_bindings(), RuntimeConfig::new(fixed_clock));
    let capabilities = runtime.handle.capabilities().snapshot();

    for capability in [
        CapabilityId::PROCESS_LIST,
        CapabilityId::PROCESS_CONTROL,
        CapabilityId::PROCESS_INSIGHTS_NETWORK,
        CapabilityId::PROCESS_INSIGHTS_GPU,
        CapabilityId::PROCESS_INSIGHTS_RESOURCES,
        CapabilityId::PROCESS_INSIGHTS_ISOLATION,
        CapabilityId::PROCESS_INSIGHTS_THREADS,
        CapabilityId::PROCESS_INSIGHTS_OPEN_FILES,
        CapabilityId::PROCESS_INSIGHTS_ENVIRONMENT,
        CapabilityId::PROCESS_AFFINITY,
        CapabilityId::PROCESS_AFFINITY_CONTROL,
        CapabilityId::PROCESS_RESOURCE_CONTROL,
    ] {
        assert_eq!(
            capabilities
                .get(&capability)
                .map(|descriptor| descriptor.providers.clone()),
            Some(vec![registered_process_provider(&capability)])
        );
    }
}

fn spawn_fixture(state: FixtureState) -> (taskmanager_application::PlatformHandle, FixtureState) {
    let runtime = crate::ChannelRuntime::new(process_bindings(), RuntimeConfig::new(fixed_clock));
    let crate::ChannelRuntime {
        handle,
        publisher,
        lanes,
    } = runtime;
    let list_state = state.clone();
    let telemetry_state = state.clone();
    let control_state = state.clone();
    let executors = ProcessExecutors::new(
        ProcessObservationExecutors::new(
            move |observed_at_ms| {
                if let Ok(mut times) = list_state.list_times.lock() {
                    times.push(observed_at_ms);
                }
                Ok(PartialSourceSnapshot::new(
                    vec![ProcessItem::new(42, "fixture")],
                    vec![SourceStatus {
                        provider: ProviderId::borrowed("fixture.process.list"),
                        outcome: SourceOutcome::Available,
                        item_count: 1,
                    }],
                ))
            },
            move |target: FrozenProcessIdentity, observed_at_ms| {
                if let Ok(mut times) = telemetry_state.telemetry_times.lock() {
                    times.push(observed_at_ms);
                }
                Ok(ProcessInsightSnapshot {
                    identity: ProcessIdentity {
                        pid: target.pid,
                        start_token: target.start_time_secs,
                    },
                    value: ProcessNetworkSnapshot {
                        state: DeviceState::healthy(observed_at_ms),
                        ..ProcessNetworkSnapshot::default()
                    },
                })
            },
            |target: FrozenProcessIdentity, _| {
                Ok(ProcessInsightSnapshot {
                    identity: ProcessIdentity {
                        pid: target.pid,
                        start_token: target.start_time_secs,
                    },
                    value: ProcessGpuSnapshot::default(),
                })
            },
            |target: FrozenProcessIdentity, _| {
                Ok(ProcessInsightSnapshot {
                    identity: ProcessIdentity {
                        pid: target.pid,
                        start_token: target.start_time_secs,
                    },
                    value: ProcessResourceSnapshot::default(),
                })
            },
            |target: FrozenProcessIdentity, _| {
                Ok(ProcessInsightSnapshot {
                    identity: ProcessIdentity {
                        pid: target.pid,
                        start_token: target.start_time_secs,
                    },
                    value: ProcessIsolation::default(),
                })
            },
            |target: FrozenProcessIdentity, observed_at_ms| {
                Ok(ProcessInsightSnapshot {
                    identity: ProcessIdentity {
                        pid: target.pid,
                        start_token: target.start_time_secs,
                    },
                    value: ProcessThreads {
                        state: DeviceState::healthy(observed_at_ms),
                        threads: Vec::new(),
                    },
                })
            },
            |_target| Ok(vec![0, 2]),
        )
        .with_open_files(|target: FrozenProcessIdentity, observed_at_ms| {
            Ok(ProcessInsightSnapshot {
                identity: ProcessIdentity {
                    pid: target.pid,
                    start_token: target.start_time_secs,
                },
                value: ProcessOpenFiles {
                    state: DeviceState::healthy(observed_at_ms),
                    ..ProcessOpenFiles::default()
                },
            })
        })
        .with_environment(|target: FrozenProcessIdentity, observed_at_ms| {
            Ok(ProcessInsightSnapshot {
                identity: ProcessIdentity {
                    pid: target.pid,
                    start_token: target.start_time_secs,
                },
                value: ProcessEnvironment {
                    state: DeviceState::healthy(observed_at_ms),
                    ..ProcessEnvironment::default()
                },
            })
        }),
        ProcessControlExecutors::new(
            |_target, _cpus| Ok(()),
            move |request| match request {
                ProcessControlRequest::ExecuteBatch(intent) => {
                    if let Ok(mut batches) = control_state.batches.lock() {
                        batches.push(intent.clone());
                    }
                    let targets = intent
                        .targets
                        .iter()
                        .cloned()
                        .map(|target| (target, ProcessBatchTargetResult::Applied))
                        .collect();
                    Ok(ProcessControlCompletion::Batch(ProcessBatchResult {
                        intent,
                        targets,
                    }))
                }
                ProcessControlRequest::EndTask(target) => {
                    Ok(ProcessControlCompletion::EndTask(target))
                }
                ProcessControlRequest::SendSignal { target, signal } => {
                    Ok(ProcessControlCompletion::Signal { target, signal })
                }
                // The fixture mirrors the adapter mapping: the neutral
                // suspend/resume concepts complete as stop/continue signals.
                ProcessControlRequest::Suspend { target } => Ok(ProcessControlCompletion::Signal {
                    target,
                    signal: ProcessSignal::Stop,
                }),
                ProcessControlRequest::Resume { target } => Ok(ProcessControlCompletion::Signal {
                    target,
                    signal: ProcessSignal::Continue,
                }),
            },
            |_target, _limits: ResourceGroupLimitRequest| Ok(()),
            || Ok(()),
        ),
    );
    let workers = crate::WorkerRuntime::default();
    spawn_process_lanes(
        &workers,
        lanes
            .process
            .try_complete()
            .expect("all process lanes are bound as one domain group"),
        executors,
        publisher,
        fixed_clock,
    )
    .expect("process workers start");
    (handle.with_runtime_lifetime(workers), state)
}

#[test]
fn shared_process_runtime_injects_clock_into_source_rich_list_executor() {
    let (handle, state) = spawn_fixture(FixtureState::default());
    handle
        .process_list()
        .expect("process list port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(3).expect("fixture request id"),
            capability: CapabilityId::PROCESS_LIST,
            submitted_at_ms: 1,
            payload: ProcessListRequest::Refresh,
        })
        .expect("list request accepted");

    let event = wait_event(&handle);
    assert_registered_process_provider(&event);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::Processes(ProcessEvent::Snapshot(ref items)))
            if items.first().is_some_and(|item| item.pid == 42)
    ));
    assert_eq!(
        state.list_times.lock().expect("list times").as_slice(),
        [CLOCK_MS]
    );
}

#[test]
fn affinity_observation_uses_its_own_event_stream_and_typed_capability() {
    let (handle, _) = spawn_fixture(FixtureState::default());
    let target = frozen_process();
    handle
        .process_affinity()
        .expect("process affinity port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(4).expect("fixture request id"),
            capability: CapabilityId::PROCESS_AFFINITY,
            submitted_at_ms: 1,
            payload: ProcessAffinityRequest {
                target: target.clone(),
            },
        })
        .expect("affinity request accepted");

    let event = wait_event(&handle);
    assert_eq!(event.capability, CapabilityId::PROCESS_AFFINITY);
    assert_registered_process_provider(&event);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::ProcessAffinity(ProcessAffinityEvent::Snapshot {
            target: ref observed,
            ref cpus,
        })) if observed == &target && cpus == &[0, 2]
    ));
}

#[test]
fn pending_process_lane_group_promotes_all_process_capabilities_atomically() {
    let complete = crate::ChannelRuntime::new(process_bindings(), RuntimeConfig::new(fixed_clock));
    assert_eq!(
        complete.lanes.process.missing_capabilities().count(),
        0,
        "a fully bound process family must carry no hidden missing lane"
    );
    assert!(
        complete.lanes.process.try_complete().is_some(),
        "the channel-owned process group must promote without native reconstruction"
    );

    let mut incomplete_bindings = process_bindings();
    incomplete_bindings.process.affinity_control = ProviderBinding::absent();
    let incomplete =
        crate::ChannelRuntime::new(incomplete_bindings, RuntimeConfig::new(fixed_clock));
    assert_eq!(
        incomplete
            .lanes
            .process
            .missing_capabilities()
            .collect::<Vec<_>>(),
        [CapabilityId::PROCESS_AFFINITY_CONTROL]
    );
    assert!(
        incomplete.lanes.process.try_complete().is_none(),
        "a process group must not promote with one typed capability missing"
    );
}

fn frozen_process() -> FrozenProcessIdentity {
    FrozenProcessIdentity::from_authoritative_parts(42, "fixture", 99, 9_900)
        .expect("fixture identity")
}

fn wait_event(
    handle: &taskmanager_application::PlatformHandle,
) -> taskmanager_application::EventEnvelope<PlatformEvent> {
    for _ in 0..100 {
        if let Some(event) = handle.events().try_recv().expect("connected event port") {
            return event;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("process runtime event did not arrive");
}

#[test]
fn shared_process_runtime_routes_batch_intent_and_typed_completion() {
    let (handle, state) = spawn_fixture(FixtureState::default());
    let intent = ProcessBatchIntent {
        action: ProcessBatchAction::Suspend,
        scope: Default::default(),
        targets: vec![frozen_process()],
    };
    handle
        .process_control()
        .expect("process control port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(1).expect("fixture request id"),
            capability: CapabilityId::PROCESS_CONTROL,
            submitted_at_ms: 1,
            payload: ProcessControlRequest::ExecuteBatch(intent.clone()),
        })
        .expect("batch request accepted");

    let event = wait_event(&handle);
    assert_registered_process_provider(&event);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::Processes(ProcessEvent::BatchCompleted(ref result)))
            if result.intent == intent
                && result.targets == vec![(frozen_process(), ProcessBatchTargetResult::Applied)]
    ));
    assert_eq!(
        state.batches.lock().expect("recorded batches").as_slice(),
        [intent],
        "the shared runtime must preserve the frozen batch intent at the native executor boundary"
    );
}

#[test]
fn resource_control_uses_its_registered_provider_and_independent_lane() {
    let (handle, _) = spawn_fixture(FixtureState::default());
    let target = frozen_process();
    let limits = ResourceGroupLimitRequest::default();
    handle
        .process_resource_control()
        .expect("process resource-control port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(8).expect("fixture request id"),
            capability: CapabilityId::PROCESS_RESOURCE_CONTROL,
            submitted_at_ms: 1,
            payload: ProcessResourceControlRequest {
                target: target.clone(),
                limits,
            },
        })
        .expect("resource-control request accepted");

    let event = wait_event(&handle);
    assert_eq!(event.capability, CapabilityId::PROCESS_RESOURCE_CONTROL);
    assert_registered_process_provider(&event);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::Processes(ProcessEvent::ResourceLimitsApplied {
            target: ref observed,
            limits: observed_limits,
        })) if observed == &target && observed_limits == limits
    ));
}

#[test]
fn shared_process_runtime_injects_clock_into_network_executor() {
    let (handle, state) = spawn_fixture(FixtureState::default());
    handle
        .process_network()
        .expect("process network port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(2).expect("fixture request id"),
            capability: CapabilityId::PROCESS_INSIGHTS_NETWORK,
            submitted_at_ms: 1,
            payload: ProcessNetworkRequest {
                target: frozen_process(),
                revision: ProcessInsightsRevision::new(7),
            },
        })
        .expect("telemetry request accepted");

    let event = wait_event(&handle);
    assert_registered_process_provider(&event);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Network(ref observation)))
            if observation.snapshot.value.state.last_success_ms == Some(CLOCK_MS)
                && observation.revision == ProcessInsightsRevision::new(7)
    ));
    assert_eq!(
        state
            .telemetry_times
            .lock()
            .expect("telemetry times")
            .as_slice(),
        [CLOCK_MS]
    );
}

#[test]
fn shared_process_runtime_routes_threads_as_an_independent_insight() {
    let (handle, _) = spawn_fixture(FixtureState::default());
    handle
        .process_threads()
        .expect("process threads port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(8).expect("fixture request id"),
            capability: CapabilityId::PROCESS_INSIGHTS_THREADS,
            submitted_at_ms: 1,
            payload: ProcessThreadsRequest {
                target: frozen_process(),
                revision: ProcessInsightsRevision::new(9),
            },
        })
        .expect("threads request accepted");

    let event = wait_event(&handle);
    assert_registered_process_provider(&event);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Threads(ref observation)))
            if observation.snapshot.value.state.last_success_ms == Some(CLOCK_MS)
                && observation.revision == ProcessInsightsRevision::new(9)
    ));
}

#[test]
fn shared_process_runtime_routes_open_files_as_an_independent_insight() {
    let (handle, _) = spawn_fixture(FixtureState::default());
    handle
        .process_open_files()
        .expect("process open_files port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(9).expect("fixture request id"),
            capability: CapabilityId::PROCESS_INSIGHTS_OPEN_FILES,
            submitted_at_ms: 1,
            payload: ProcessOpenFilesRequest {
                target: frozen_process(),
                revision: ProcessInsightsRevision::new(11),
            },
        })
        .expect("open_files request accepted");

    let event = wait_event(&handle);
    assert_eq!(event.capability, CapabilityId::PROCESS_INSIGHTS_OPEN_FILES);
    assert_registered_process_provider(&event);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::OpenFiles(ref observation)))
            if observation.snapshot.value.state.last_success_ms == Some(CLOCK_MS)
                && observation.revision == ProcessInsightsRevision::new(11)
    ));
}

#[test]
fn shared_process_runtime_routes_environment_as_an_independent_insight() {
    let (handle, _) = spawn_fixture(FixtureState::default());
    handle
        .process_environment()
        .expect("process environment port")
        .try_submit(RequestEnvelope {
            id: RequestId::new(10).expect("fixture request id"),
            capability: CapabilityId::PROCESS_INSIGHTS_ENVIRONMENT,
            submitted_at_ms: 1,
            payload: ProcessEnvironmentRequest {
                target: frozen_process(),
                revision: ProcessInsightsRevision::new(12),
            },
        })
        .expect("environment request accepted");

    let event = wait_event(&handle);
    assert_eq!(event.capability, CapabilityId::PROCESS_INSIGHTS_ENVIRONMENT);
    assert_registered_process_provider(&event);
    assert!(matches!(
        event.outcome,
        Ok(PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Environment(
            ref observation
        ))) if observation.snapshot.value.state.last_success_ms == Some(CLOCK_MS)
            && observation.revision == ProcessInsightsRevision::new(12)
    ));
}
