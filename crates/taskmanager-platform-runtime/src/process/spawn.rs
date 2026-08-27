//! Process-lane worker startup and event routing.

use std::sync::Arc;

use taskmanager_application::{
    PlatformEvent, ProcessAffinityControlRequest, ProcessAffinityEvent, ProcessAffinityRequest,
    ProcessEnvironmentRequest, ProcessEvent, ProcessGpuRequest, ProcessInsightFacetEvent,
    ProcessInsightObservation, ProcessIsolationRequest, ProcessListRequest,
    ProcessNetworkEscalationRequest, ProcessNetworkRequest, ProcessOpenFilesRequest,
    ProcessResourceControlRequest, ProcessResourcesRequest, ProcessThreadsRequest,
};

use super::{ProcessControlLanes, ProcessExecutors, ProcessObservationLanes, ProcessRuntimeLanes};
use crate::{
    RuntimeEventPublisher, WorkerRuntime, WorkerSpawnError, spawn_lane, spawn_observation_lane,
};

/// Attach all process executors to their independent typed lanes.
pub fn spawn_process_lanes(
    workers: &WorkerRuntime,
    lanes: ProcessRuntimeLanes,
    executors: ProcessExecutors,
    events: Arc<RuntimeEventPublisher>,
    clock_ms: fn() -> u64,
) -> Result<(), WorkerSpawnError> {
    let ProcessRuntimeLanes {
        observations:
            ProcessObservationLanes {
                list,
                network,
                gpu,
                resources,
                isolation,
                threads,
                affinity,
                open_files,
                environment,
            },
        controls:
            ProcessControlLanes {
                affinity_control,
                resource_control,
                control,
                network_escalation,
            },
    } = lanes;
    let ProcessExecutors {
        list: mut execute_list,
        network: mut execute_network,
        gpu: mut execute_gpu,
        resources: mut execute_resources,
        isolation: mut execute_isolation,
        threads: mut execute_threads,
        affinity: mut execute_affinity,
        affinity_control: mut execute_affinity_control,
        resource_control: mut execute_resource_control,
        control: mut execute_control,
        network_escalation: mut execute_network_escalation,
        open_files: execute_open_files,
        environment: execute_environment,
    } = executors;

    spawn_observation_lane(
        workers,
        list,
        events.clone(),
        move |ProcessListRequest::Refresh| execute_list(clock_ms()),
        |snapshot| PlatformEvent::Processes(ProcessEvent::Snapshot(snapshot.items)),
    )?;
    spawn_observation_lane(
        workers,
        network,
        events.clone(),
        move |ProcessNetworkRequest { target, revision }| {
            let snapshot = execute_network(target.clone(), clock_ms())?;
            Ok(ProcessInsightObservation {
                target,
                revision,
                snapshot,
            })
        },
        |observation| {
            PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Network(Box::new(
                observation,
            )))
        },
    )?;
    spawn_observation_lane(
        workers,
        gpu,
        events.clone(),
        move |ProcessGpuRequest { target, revision }| {
            let snapshot = execute_gpu(target.clone(), clock_ms())?;
            Ok(ProcessInsightObservation {
                target,
                revision,
                snapshot,
            })
        },
        |observation| {
            PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Gpu(Box::new(observation)))
        },
    )?;
    spawn_observation_lane(
        workers,
        resources,
        events.clone(),
        move |ProcessResourcesRequest { target, revision }| {
            let snapshot = execute_resources(target.clone(), clock_ms())?;
            Ok(ProcessInsightObservation {
                target,
                revision,
                snapshot,
            })
        },
        |observation| {
            PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Resources(Box::new(
                observation,
            )))
        },
    )?;
    spawn_observation_lane(
        workers,
        isolation,
        events.clone(),
        move |ProcessIsolationRequest { target, revision }| {
            let snapshot = execute_isolation(target.clone(), clock_ms())?;
            Ok(ProcessInsightObservation {
                target,
                revision,
                snapshot,
            })
        },
        |observation| {
            PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Isolation(Box::new(
                observation,
            )))
        },
    )?;
    spawn_observation_lane(
        workers,
        threads,
        events.clone(),
        move |ProcessThreadsRequest { target, revision }| {
            let snapshot = execute_threads(target.clone(), clock_ms())?;
            Ok(ProcessInsightObservation {
                target,
                revision,
                snapshot,
            })
        },
        |observation| {
            PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Threads(Box::new(
                observation,
            )))
        },
    )?;
    if let (Some(open_files), Some(mut execute_open_files)) = (open_files, execute_open_files) {
        spawn_observation_lane(
            workers,
            open_files,
            events.clone(),
            move |ProcessOpenFilesRequest { target, revision }| {
                let snapshot = execute_open_files(target.clone(), clock_ms())?;
                Ok(ProcessInsightObservation {
                    target,
                    revision,
                    snapshot,
                })
            },
            |observation| {
                PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::OpenFiles(Box::new(
                    observation,
                )))
            },
        )?;
    }
    if let (Some(environment), Some(mut execute_environment)) = (environment, execute_environment) {
        spawn_observation_lane(
            workers,
            environment,
            events.clone(),
            move |ProcessEnvironmentRequest { target, revision }| {
                let snapshot = execute_environment(target.clone(), clock_ms())?;
                Ok(ProcessInsightObservation {
                    target,
                    revision,
                    snapshot,
                })
            },
            |observation| {
                PlatformEvent::ProcessInsightFacet(ProcessInsightFacetEvent::Environment(Box::new(
                    observation,
                )))
            },
        )?;
    }
    spawn_lane(
        workers,
        affinity,
        events.clone(),
        move |ProcessAffinityRequest { target }| {
            let cpus = execute_affinity(target.clone())?;
            Ok(PlatformEvent::ProcessAffinity(
                ProcessAffinityEvent::Snapshot { target, cpus },
            ))
        },
    )?;
    spawn_lane(
        workers,
        affinity_control,
        events.clone(),
        move |ProcessAffinityControlRequest { target, cpus }| {
            execute_affinity_control(target.clone(), cpus.clone())?;
            Ok(PlatformEvent::Processes(ProcessEvent::AffinityApplied {
                target,
                cpus,
            }))
        },
    )?;
    spawn_lane(
        workers,
        resource_control,
        events.clone(),
        move |ProcessResourceControlRequest { target, limits }| {
            execute_resource_control(target.clone(), limits)?;
            Ok(PlatformEvent::Processes(
                ProcessEvent::ResourceLimitsApplied { target, limits },
            ))
        },
    )?;
    spawn_lane(
        workers,
        network_escalation,
        events.clone(),
        move |ProcessNetworkEscalationRequest| {
            execute_network_escalation()?;
            Ok(PlatformEvent::Processes(
                ProcessEvent::NetworkCaptureEscalated,
            ))
        },
    )?;
    spawn_lane(workers, control, events, move |request| {
        Ok(PlatformEvent::Processes(
            execute_control(request)?.into_event(),
        ))
    })
}
