//! OS-neutral process execution contracts and typed lane routing.

use crossbeam_channel::Receiver;
use taskmanager_application::{
    CapabilityId, PartialSourceSnapshot, ProcessAffinityControlRequest, ProcessAffinityRequest,
    ProcessControlRequest, ProcessEnvironmentRequest, ProcessEvent, ProcessGpuRequest,
    ProcessIsolationRequest, ProcessListRequest, ProcessNetworkEscalationRequest,
    ProcessNetworkRequest, ProcessOpenFilesRequest, ProcessResourceControlRequest,
    ProcessResourcesRequest, ProcessThreadsRequest, ProviderFailure,
};
use taskmanager_core::{
    FrozenProcessIdentity, ProcessBatchResult, ProcessEnvironment, ProcessGpuSnapshot,
    ProcessInsightSnapshot, ProcessIsolation, ProcessItem, ProcessNetworkSnapshot,
    ProcessOpenFiles, ProcessResourceSnapshot, ProcessSignal, ProcessThreads,
    ResourceGroupLimitRequest,
};

use crate::Queued;

mod spawn;

pub use spawn::spawn_process_lanes;

type ListExecutor =
    dyn FnMut(u64) -> Result<PartialSourceSnapshot<ProcessItem>, ProviderFailure> + Send + 'static;
type NetworkExecutor = dyn FnMut(
        FrozenProcessIdentity,
        u64,
    ) -> Result<ProcessInsightSnapshot<ProcessNetworkSnapshot>, ProviderFailure>
    + Send
    + 'static;
type GpuExecutor = dyn FnMut(
        FrozenProcessIdentity,
        u64,
    ) -> Result<ProcessInsightSnapshot<ProcessGpuSnapshot>, ProviderFailure>
    + Send
    + 'static;
type ResourcesExecutor = dyn FnMut(
        FrozenProcessIdentity,
        u64,
    ) -> Result<ProcessInsightSnapshot<ProcessResourceSnapshot>, ProviderFailure>
    + Send
    + 'static;
type IsolationExecutor = dyn FnMut(
        FrozenProcessIdentity,
        u64,
    ) -> Result<ProcessInsightSnapshot<ProcessIsolation>, ProviderFailure>
    + Send
    + 'static;
type ThreadsExecutor = dyn FnMut(
        FrozenProcessIdentity,
        u64,
    ) -> Result<ProcessInsightSnapshot<ProcessThreads>, ProviderFailure>
    + Send
    + 'static;
type OpenFilesExecutor = dyn FnMut(
        FrozenProcessIdentity,
        u64,
    ) -> Result<ProcessInsightSnapshot<ProcessOpenFiles>, ProviderFailure>
    + Send
    + 'static;
type EnvironmentExecutor = dyn FnMut(
        FrozenProcessIdentity,
        u64,
    ) -> Result<ProcessInsightSnapshot<ProcessEnvironment>, ProviderFailure>
    + Send
    + 'static;
type AffinityExecutor =
    dyn FnMut(FrozenProcessIdentity) -> Result<Vec<u32>, ProviderFailure> + Send + 'static;
type AffinityControlExecutor =
    dyn FnMut(FrozenProcessIdentity, Vec<u32>) -> Result<(), ProviderFailure> + Send + 'static;
type ResourceControlExecutor = dyn FnMut(FrozenProcessIdentity, ResourceGroupLimitRequest) -> Result<(), ProviderFailure>
    + Send
    + 'static;
type NetworkEscalationExecutor = dyn FnMut() -> Result<(), ProviderFailure> + Send + 'static;
type ControlExecutor = dyn FnMut(ProcessControlRequest) -> Result<ProcessControlCompletion, ProviderFailure>
    + Send
    + 'static;

/// Provider-neutral completion returned by the native process-control adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessControlCompletion {
    EndTask(FrozenProcessIdentity),
    Batch(ProcessBatchResult),
    Signal {
        target: FrozenProcessIdentity,
        signal: ProcessSignal,
    },
}

impl ProcessControlCompletion {
    fn into_event(self) -> ProcessEvent {
        match self {
            Self::EndTask(target) => ProcessEvent::EndTaskCompleted(target),
            Self::Batch(result) => ProcessEvent::BatchCompleted(result),
            Self::Signal { target, signal } => ProcessEvent::SignalCompleted { target, signal },
        }
    }
}

/// Native process operations adapted into OS-independent executor closures.
///
/// The runtime owns request and event semantics without depending on any
/// native provider SPI. An OS adapter performs only provider-to-closure
/// delegation at its composition edge.
pub struct ProcessExecutors {
    list: Box<ListExecutor>,
    network: Box<NetworkExecutor>,
    gpu: Box<GpuExecutor>,
    resources: Box<ResourcesExecutor>,
    isolation: Box<IsolationExecutor>,
    threads: Box<ThreadsExecutor>,
    affinity: Box<AffinityExecutor>,
    affinity_control: Box<AffinityControlExecutor>,
    resource_control: Box<ResourceControlExecutor>,
    network_escalation: Box<NetworkEscalationExecutor>,
    control: Box<ControlExecutor>,
    open_files: Option<Box<OpenFilesExecutor>>,
    environment: Option<Box<EnvironmentExecutor>>,
}

/// Read-only process executors that may run on independent observation lanes.
pub struct ProcessObservationExecutors {
    list: Box<ListExecutor>,
    network: Box<NetworkExecutor>,
    gpu: Box<GpuExecutor>,
    resources: Box<ResourcesExecutor>,
    isolation: Box<IsolationExecutor>,
    threads: Box<ThreadsExecutor>,
    affinity: Box<AffinityExecutor>,
    open_files: Option<Box<OpenFilesExecutor>>,
    environment: Option<Box<EnvironmentExecutor>>,
}

impl ProcessObservationExecutors {
    #[must_use]
    pub fn new<L, N, G, R, I, T, A>(
        list: L,
        network: N,
        gpu: G,
        resources: R,
        isolation: I,
        threads: T,
        affinity: A,
    ) -> Self
    where
        L: FnMut(u64) -> Result<PartialSourceSnapshot<ProcessItem>, ProviderFailure>
            + Send
            + 'static,
        N: FnMut(
                FrozenProcessIdentity,
                u64,
            )
                -> Result<ProcessInsightSnapshot<ProcessNetworkSnapshot>, ProviderFailure>
            + Send
            + 'static,
        G: FnMut(
                FrozenProcessIdentity,
                u64,
            )
                -> Result<ProcessInsightSnapshot<ProcessGpuSnapshot>, ProviderFailure>
            + Send
            + 'static,
        R: FnMut(
                FrozenProcessIdentity,
                u64,
            )
                -> Result<ProcessInsightSnapshot<ProcessResourceSnapshot>, ProviderFailure>
            + Send
            + 'static,
        I: FnMut(
                FrozenProcessIdentity,
                u64,
            ) -> Result<ProcessInsightSnapshot<ProcessIsolation>, ProviderFailure>
            + Send
            + 'static,
        T: FnMut(
                FrozenProcessIdentity,
                u64,
            ) -> Result<ProcessInsightSnapshot<ProcessThreads>, ProviderFailure>
            + Send
            + 'static,
        A: FnMut(FrozenProcessIdentity) -> Result<Vec<u32>, ProviderFailure> + Send + 'static,
    {
        Self {
            list: Box::new(list),
            network: Box::new(network),
            gpu: Box::new(gpu),
            resources: Box::new(resources),
            isolation: Box::new(isolation),
            threads: Box::new(threads),
            affinity: Box::new(affinity),
            open_files: None,
            environment: None,
        }
    }

    /// Attach the optional OpenFiles observation executor. Native adapters
    /// without this facet leave it absent; the runtime spawns no lane and the
    /// application port stays `None` (typed `Unsupported` on submission).
    #[must_use]
    pub fn with_open_files<F>(mut self, open_files: F) -> Self
    where
        F: FnMut(
                FrozenProcessIdentity,
                u64,
            ) -> Result<ProcessInsightSnapshot<ProcessOpenFiles>, ProviderFailure>
            + Send
            + 'static,
    {
        self.open_files = Some(Box::new(open_files));
        self
    }

    /// Attach the optional Environment observation executor. Native adapters
    /// without this facet leave it absent; the runtime spawns no lane and the
    /// application port stays `None` (typed `Unsupported` on submission).
    #[must_use]
    pub fn with_environment<F>(mut self, environment: F) -> Self
    where
        F: FnMut(
                FrozenProcessIdentity,
                u64,
            )
                -> Result<ProcessInsightSnapshot<ProcessEnvironment>, ProviderFailure>
            + Send
            + 'static,
    {
        self.environment = Some(Box::new(environment));
        self
    }
}

/// Mutating process executors kept separate from read-only observations.
pub struct ProcessControlExecutors {
    affinity_control: Box<AffinityControlExecutor>,
    resource_control: Box<ResourceControlExecutor>,
    control: Box<ControlExecutor>,
    network_escalation: Box<NetworkEscalationExecutor>,
}

impl ProcessControlExecutors {
    #[must_use]
    pub fn new<M, C, R, E>(
        affinity_control: M,
        control: C,
        resource_control: R,
        network_escalation: E,
    ) -> Self
    where
        M: FnMut(FrozenProcessIdentity, Vec<u32>) -> Result<(), ProviderFailure> + Send + 'static,
        C: FnMut(ProcessControlRequest) -> Result<ProcessControlCompletion, ProviderFailure>
            + Send
            + 'static,
        R: FnMut(FrozenProcessIdentity, ResourceGroupLimitRequest) -> Result<(), ProviderFailure>
            + Send
            + 'static,
        E: FnMut() -> Result<(), ProviderFailure> + Send + 'static,
    {
        Self {
            affinity_control: Box::new(affinity_control),
            resource_control: Box::new(resource_control),
            control: Box::new(control),
            network_escalation: Box::new(network_escalation),
        }
    }
}

impl ProcessExecutors {
    #[must_use]
    pub fn new(
        observations: ProcessObservationExecutors,
        controls: ProcessControlExecutors,
    ) -> Self {
        let ProcessObservationExecutors {
            list,
            network,
            gpu,
            resources,
            isolation,
            threads,
            affinity,
            open_files,
            environment,
        } = observations;
        let ProcessControlExecutors {
            affinity_control,
            resource_control,
            control,
            network_escalation,
        } = controls;
        Self {
            list,
            network,
            gpu,
            resources,
            isolation,
            threads,
            affinity,
            affinity_control,
            resource_control,
            control,
            network_escalation,
            open_files,
            environment,
        }
    }
}

/// Optional provider-side receivers while the native adapter capability set is
/// still being assembled.
///
/// Keeping the eleven typed lanes in one domain group lets channel construction,
/// completeness validation, and native composition transfer the same ownership
/// unit without flattening and reconstructing it at every boundary.
pub struct PendingProcessRuntimeLanes {
    pub observations: PendingProcessObservationLanes,
    pub controls: PendingProcessControlLanes,
}

pub struct PendingProcessObservationLanes {
    pub list_rx: Option<Receiver<Queued<ProcessListRequest>>>,
    pub network_rx: Option<Receiver<Queued<ProcessNetworkRequest>>>,
    pub gpu_rx: Option<Receiver<Queued<ProcessGpuRequest>>>,
    pub resources_rx: Option<Receiver<Queued<ProcessResourcesRequest>>>,
    pub isolation_rx: Option<Receiver<Queued<ProcessIsolationRequest>>>,
    pub threads_rx: Option<Receiver<Queued<ProcessThreadsRequest>>>,
    pub affinity_rx: Option<Receiver<Queued<ProcessAffinityRequest>>>,
    pub open_files_rx: Option<Receiver<Queued<ProcessOpenFilesRequest>>>,
    pub environment_rx: Option<Receiver<Queued<ProcessEnvironmentRequest>>>,
}

pub struct PendingProcessControlLanes {
    pub affinity_control_rx: Option<Receiver<Queued<ProcessAffinityControlRequest>>>,
    pub resource_control_rx: Option<Receiver<Queued<ProcessResourceControlRequest>>>,
    pub control_rx: Option<Receiver<Queued<ProcessControlRequest>>>,
    pub network_escalation_rx: Option<Receiver<Queued<ProcessNetworkEscalationRequest>>>,
}

impl PendingProcessControlLanes {
    #[must_use]
    pub(crate) fn new(
        affinity_control_rx: Option<Receiver<Queued<ProcessAffinityControlRequest>>>,
        resource_control_rx: Option<Receiver<Queued<ProcessResourceControlRequest>>>,
        control_rx: Option<Receiver<Queued<ProcessControlRequest>>>,
        network_escalation_rx: Option<Receiver<Queued<ProcessNetworkEscalationRequest>>>,
    ) -> Self {
        Self {
            affinity_control_rx,
            resource_control_rx,
            control_rx,
            network_escalation_rx,
        }
    }
}

impl PendingProcessRuntimeLanes {
    #[must_use]
    pub(crate) fn new(
        observations: PendingProcessObservationLanes,
        controls: PendingProcessControlLanes,
    ) -> Self {
        Self {
            observations,
            controls,
        }
    }

    pub(crate) fn missing_capabilities(&self) -> impl Iterator<Item = CapabilityId> {
        [
            (
                self.observations.list_rx.is_none(),
                CapabilityId::PROCESS_LIST,
            ),
            (
                self.observations.network_rx.is_none(),
                CapabilityId::PROCESS_INSIGHTS_NETWORK,
            ),
            (
                self.observations.gpu_rx.is_none(),
                CapabilityId::PROCESS_INSIGHTS_GPU,
            ),
            (
                self.observations.resources_rx.is_none(),
                CapabilityId::PROCESS_INSIGHTS_RESOURCES,
            ),
            (
                self.observations.isolation_rx.is_none(),
                CapabilityId::PROCESS_INSIGHTS_ISOLATION,
            ),
            (
                self.observations.threads_rx.is_none(),
                CapabilityId::PROCESS_INSIGHTS_THREADS,
            ),
            (
                self.observations.affinity_rx.is_none(),
                CapabilityId::PROCESS_AFFINITY,
            ),
            (
                self.controls.affinity_control_rx.is_none(),
                CapabilityId::PROCESS_AFFINITY_CONTROL,
            ),
            (
                self.controls.resource_control_rx.is_none(),
                CapabilityId::PROCESS_RESOURCE_CONTROL,
            ),
            (
                self.controls.network_escalation_rx.is_none(),
                CapabilityId::PROCESS_NETWORK_ESCALATION,
            ),
            (
                self.controls.control_rx.is_none(),
                CapabilityId::PROCESS_CONTROL,
            ),
        ]
        .into_iter()
        .filter_map(|(is_missing, capability)| is_missing.then_some(capability))
    }

    /// Promote the process group only when all eleven independent typed lanes
    /// are bound.
    #[must_use]
    pub fn try_complete(self) -> Option<ProcessRuntimeLanes> {
        let Self {
            observations:
                PendingProcessObservationLanes {
                    list_rx: Some(list),
                    network_rx: Some(network),
                    gpu_rx: Some(gpu),
                    resources_rx: Some(resources),
                    isolation_rx: Some(isolation),
                    threads_rx: Some(threads),
                    affinity_rx: Some(affinity),
                    open_files_rx,
                    environment_rx,
                },
            controls:
                PendingProcessControlLanes {
                    affinity_control_rx: Some(affinity_control),
                    resource_control_rx: Some(resource_control),
                    control_rx: Some(control),
                    network_escalation_rx: Some(network_escalation),
                },
        } = self
        else {
            return None;
        };
        Some(ProcessRuntimeLanes::new(
            ProcessObservationLanes {
                list,
                network,
                gpu,
                resources,
                isolation,
                threads,
                affinity,
                open_files: open_files_rx,
                environment: environment_rx,
            },
            ProcessControlLanes::new(
                affinity_control,
                resource_control,
                control,
                network_escalation,
            ),
        ))
    }
}

/// Complete provider-side receivers for the process capability family.
pub struct ProcessRuntimeLanes {
    observations: ProcessObservationLanes,
    controls: ProcessControlLanes,
}

struct ProcessObservationLanes {
    list: Receiver<Queued<ProcessListRequest>>,
    network: Receiver<Queued<ProcessNetworkRequest>>,
    gpu: Receiver<Queued<ProcessGpuRequest>>,
    resources: Receiver<Queued<ProcessResourcesRequest>>,
    isolation: Receiver<Queued<ProcessIsolationRequest>>,
    threads: Receiver<Queued<ProcessThreadsRequest>>,
    affinity: Receiver<Queued<ProcessAffinityRequest>>,
    open_files: Option<Receiver<Queued<ProcessOpenFilesRequest>>>,
    environment: Option<Receiver<Queued<ProcessEnvironmentRequest>>>,
}

struct ProcessControlLanes {
    affinity_control: Receiver<Queued<ProcessAffinityControlRequest>>,
    resource_control: Receiver<Queued<ProcessResourceControlRequest>>,
    control: Receiver<Queued<ProcessControlRequest>>,
    network_escalation: Receiver<Queued<ProcessNetworkEscalationRequest>>,
}

impl ProcessControlLanes {
    #[must_use]
    fn new(
        affinity_control: Receiver<Queued<ProcessAffinityControlRequest>>,
        resource_control: Receiver<Queued<ProcessResourceControlRequest>>,
        control: Receiver<Queued<ProcessControlRequest>>,
        network_escalation: Receiver<Queued<ProcessNetworkEscalationRequest>>,
    ) -> Self {
        Self {
            affinity_control,
            resource_control,
            control,
            network_escalation,
        }
    }
}

impl ProcessRuntimeLanes {
    #[must_use]
    fn new(observations: ProcessObservationLanes, controls: ProcessControlLanes) -> Self {
        Self {
            observations,
            controls,
        }
    }
}

#[cfg(test)]
#[path = "../tests/headless/process.rs"]
mod tests;
