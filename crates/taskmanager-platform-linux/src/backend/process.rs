//! Linux process providers bound to shared process observation and control executors.
//!
//! Owns `ProcessProviders`, split into read-only `ProcessObservationProviders`
//! and mutating `ProcessControlProviders` with separate control backpressure.

use taskmanager_application::{
    ProcessAffinityControlRequest, ProcessAffinityRequest, ProcessControlRequest,
    ProcessEnvironmentRequest, ProcessGpuRequest, ProcessIsolationRequest, ProcessListRequest,
    ProcessNetworkEscalationRequest, ProcessNetworkRequest, ProcessOpenFilesRequest,
    ProcessResourceControlRequest, ProcessResourcesRequest, ProcessThreadsRequest,
};
use taskmanager_core::core::process::ProcessSignal;
use taskmanager_platform_provider::{
    ProcessAffinityControlProvider, ProcessAffinityProvider, ProcessControlProvider,
    ProcessEnvironmentProvider, ProcessGpuProvider, ProcessIsolationProvider, ProcessListProvider,
    ProcessNetworkEscalationProvider, ProcessNetworkProvider, ProcessOpenFilesProvider,
    ProcessResourceControlProvider, ProcessResourcesProvider, ProcessThreadsProvider,
};
use taskmanager_platform_runtime::{
    ProcessControlCompletion, ProcessControlExecutors, ProcessExecutors,
    ProcessObservationExecutors, ProcessProviderBindings, ProcessProviderBindingsInput,
    ProviderRegistration,
};

type ListRegistration = ProviderRegistration<ProcessListRequest, Box<dyn ProcessListProvider>>;
type ControlRegistration =
    ProviderRegistration<ProcessControlRequest, Box<dyn ProcessControlProvider>>;
type NetworkRegistration =
    ProviderRegistration<ProcessNetworkRequest, Box<dyn ProcessNetworkProvider>>;
type GpuRegistration = ProviderRegistration<ProcessGpuRequest, Box<dyn ProcessGpuProvider>>;
type ResourcesRegistration =
    ProviderRegistration<ProcessResourcesRequest, Box<dyn ProcessResourcesProvider>>;
type IsolationRegistration =
    ProviderRegistration<ProcessIsolationRequest, Box<dyn ProcessIsolationProvider>>;
type ThreadsRegistration =
    ProviderRegistration<ProcessThreadsRequest, Box<dyn ProcessThreadsProvider>>;
type OpenFilesRegistration =
    ProviderRegistration<ProcessOpenFilesRequest, Box<dyn ProcessOpenFilesProvider>>;
type EnvironmentRegistration =
    ProviderRegistration<ProcessEnvironmentRequest, Box<dyn ProcessEnvironmentProvider>>;
type AffinityRegistration =
    ProviderRegistration<ProcessAffinityRequest, Box<dyn ProcessAffinityProvider>>;
type AffinityControlRegistration =
    ProviderRegistration<ProcessAffinityControlRequest, Box<dyn ProcessAffinityControlProvider>>;
type ResourceControlRegistration =
    ProviderRegistration<ProcessResourceControlRequest, Box<dyn ProcessResourceControlProvider>>;
type NetworkEscalationRegistration = ProviderRegistration<
    ProcessNetworkEscalationRequest,
    Box<dyn ProcessNetworkEscalationProvider>,
>;

/// Linux provider implementations adapted to the shared process executors.
pub struct ProcessProviders {
    observations: ProcessObservationProviders,
    controls: ProcessControlProviders,
}

/// Read-only Linux process providers, grouped independently from mutation.
pub struct ProcessObservationProviders {
    list: ListRegistration,
    network: NetworkRegistration,
    gpu: GpuRegistration,
    resources: ResourcesRegistration,
    isolation: IsolationRegistration,
    threads: ThreadsRegistration,
    open_files: OpenFilesRegistration,
    environment: EnvironmentRegistration,
    affinity: AffinityRegistration,
}

/// Named composition input for [`ProcessObservationProviders`].
///
/// Each field is one independently typed read-only process capability, already
/// erased to the platform-neutral provider trait object the group stores.
/// Naming the nine axes keeps the capability vocabulary explicit at the
/// composition seam without a positional argument list.
pub struct ProcessObservationProvidersParams {
    /// Process list observation provider.
    pub list: ListRegistration,
    /// Per-process network byte-accounting provider.
    pub network: NetworkRegistration,
    /// Per-process GPU utilization provider.
    pub gpu: GpuRegistration,
    /// Per-process resource-usage provider.
    pub resources: ResourcesRegistration,
    /// Per-process isolation provider.
    pub isolation: IsolationRegistration,
    /// Per-process thread breakdown provider.
    pub threads: ThreadsRegistration,
    /// Per-process open-files provider.
    pub open_files: OpenFilesRegistration,
    /// Per-process environment provider.
    pub environment: EnvironmentRegistration,
    /// Per-process CPU-affinity provider.
    pub affinity: AffinityRegistration,
}

impl ProcessObservationProviders {
    #[must_use]
    pub fn new(params: ProcessObservationProvidersParams) -> Self {
        Self {
            list: params.list,
            network: params.network,
            gpu: params.gpu,
            resources: params.resources,
            isolation: params.isolation,
            threads: params.threads,
            open_files: params.open_files,
            environment: params.environment,
            affinity: params.affinity,
        }
    }

    fn into_runtime(self) -> ProcessObservationExecutors {
        let Self {
            list,
            network,
            gpu,
            resources,
            isolation,
            threads,
            open_files,
            environment,
            affinity,
        } = self;
        let mut list = list.into_provider();
        let mut network = network.into_provider();
        let mut gpu = gpu.into_provider();
        let mut resources = resources.into_provider();
        let mut isolation = isolation.into_provider();
        let mut threads = threads.into_provider();
        let mut open_files = open_files.into_provider();
        let mut environment = environment.into_provider();
        let mut affinity = affinity.into_provider();
        ProcessObservationExecutors::new(
            move |observed_at_ms| list.refresh(observed_at_ms),
            move |target, observed_at_ms| network.observe(&target, observed_at_ms),
            move |target, observed_at_ms| gpu.observe(&target, observed_at_ms),
            move |target, observed_at_ms| resources.observe(&target, observed_at_ms),
            move |target, observed_at_ms| isolation.observe(&target, observed_at_ms),
            move |target, observed_at_ms| threads.observe(&target, observed_at_ms),
            move |target| affinity.affinity(&target),
        )
        .with_open_files(move |target, observed_at_ms| open_files.observe(&target, observed_at_ms))
        .with_environment(move |target, observed_at_ms| {
            environment.observe(&target, observed_at_ms)
        })
    }
}

/// Mutating Linux process providers with separate control backpressure.
pub struct ProcessControlProviders {
    affinity_control: AffinityControlRegistration,
    resource_control: ResourceControlRegistration,
    control: ControlRegistration,
    network_escalation: NetworkEscalationRegistration,
}

impl ProcessControlProviders {
    #[must_use]
    pub fn new<M, C, R, E>(
        affinity_control: ProviderRegistration<ProcessAffinityControlRequest, M>,
        control: ProviderRegistration<ProcessControlRequest, C>,
        resource_control: ProviderRegistration<ProcessResourceControlRequest, R>,
        network_escalation: ProviderRegistration<ProcessNetworkEscalationRequest, E>,
    ) -> Self
    where
        M: ProcessAffinityControlProvider,
        C: ProcessControlProvider,
        R: ProcessResourceControlProvider,
        E: ProcessNetworkEscalationProvider,
    {
        Self {
            affinity_control: affinity_control.map_provider(|provider| {
                Box::new(provider) as Box<dyn ProcessAffinityControlProvider>
            }),
            resource_control: resource_control.map_provider(|provider| {
                Box::new(provider) as Box<dyn ProcessResourceControlProvider>
            }),
            control: control
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessControlProvider>),
            network_escalation: network_escalation.map_provider(|provider| {
                Box::new(provider) as Box<dyn ProcessNetworkEscalationProvider>
            }),
        }
    }

    fn into_runtime(self) -> ProcessControlExecutors {
        let Self {
            affinity_control,
            resource_control,
            control,
            network_escalation,
        } = self;
        let mut affinity_control = affinity_control.into_provider();
        let mut resource_control = resource_control.into_provider();
        let mut control = control.into_provider();
        let mut network_escalation = network_escalation.into_provider();
        ProcessControlExecutors::new(
            move |target, cpus| affinity_control.set_affinity(&target, &cpus),
            move |request| match request {
                ProcessControlRequest::EndTask(target) => {
                    control.end_task(target.clone())?;
                    Ok(ProcessControlCompletion::EndTask(target))
                }
                ProcessControlRequest::ExecuteBatch(intent) => Ok(ProcessControlCompletion::Batch(
                    control.execute_batch(intent)?,
                )),
                ProcessControlRequest::SendSignal { target, signal } => {
                    control.send_signal(&target, signal)?;
                    Ok(ProcessControlCompletion::Signal { target, signal })
                }
                // The neutral suspend/resume concepts map onto the Unix
                // stop/continue primitives at this adapter edge; the shared
                // signal path already performs identity validation and
                // escalation, and completion rides the signal event.
                ProcessControlRequest::Suspend { target } => {
                    control.send_signal(&target, ProcessSignal::Stop)?;
                    Ok(ProcessControlCompletion::Signal {
                        target,
                        signal: ProcessSignal::Stop,
                    })
                }
                ProcessControlRequest::Resume { target } => {
                    control.send_signal(&target, ProcessSignal::Continue)?;
                    Ok(ProcessControlCompletion::Signal {
                        target,
                        signal: ProcessSignal::Continue,
                    })
                }
            },
            move |target, limits| resource_control.apply_limits(&target, &limits),
            move || network_escalation.request_capture_escalation(),
        )
    }
}

impl ProcessProviders {
    #[must_use]
    pub const fn new(
        observations: ProcessObservationProviders,
        controls: ProcessControlProviders,
    ) -> Self {
        Self {
            observations,
            controls,
        }
    }

    pub(crate) fn runtime_bindings(&self) -> ProcessProviderBindings {
        ProcessProviderBindings::new(ProcessProviderBindingsInput {
            list: self.observations.list.binding(),
            control: self.controls.control.binding(),
            network: self.observations.network.binding(),
            gpu: self.observations.gpu.binding(),
            resources: self.observations.resources.binding(),
            isolation: self.observations.isolation.binding(),
            threads: self.observations.threads.binding(),
            affinity: self.observations.affinity.binding(),
            affinity_control: self.controls.affinity_control.binding(),
            resource_control: self.controls.resource_control.binding(),
            network_escalation: self.controls.network_escalation.binding(),
        })
        .with_open_files(&self.observations.open_files)
        .with_environment(&self.observations.environment)
    }

    pub(crate) fn into_runtime(self) -> ProcessExecutors {
        ProcessExecutors::new(
            self.observations.into_runtime(),
            self.controls.into_runtime(),
        )
    }
}
