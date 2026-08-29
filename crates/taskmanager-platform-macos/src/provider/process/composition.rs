//! macOS process provider composition (the registry-facing group structs).
//!
//! `MacProcessObservationProviders` / `MacProcessControlProviders` /
//! `MacProcessProviders` adapt the concrete providers of `process.rs` and
//! `process/pending.rs` into typed runtime registrations — the same split the
//! Linux adapter's `backend/process.rs` uses, with the optional OpenFiles
//! facet attached through its own builder so absence stays a typed
//! `Unsupported` instead of an absent descriptor.

use taskmanager_application::{
    ProcessAffinityControlRequest, ProcessAffinityRequest, ProcessControlRequest,
    ProcessGpuRequest, ProcessIsolationRequest, ProcessListRequest,
    ProcessNetworkEscalationRequest, ProcessNetworkRequest, ProcessOpenFilesRequest,
    ProcessResourceControlRequest, ProcessResourcesRequest, ProcessThreadsRequest,
};
use taskmanager_core::core::process::ProcessSignal;
use taskmanager_platform_provider::{
    ProcessAffinityControlProvider, ProcessAffinityProvider, ProcessControlProvider,
    ProcessGpuProvider, ProcessIsolationProvider, ProcessListProvider,
    ProcessNetworkEscalationProvider, ProcessNetworkProvider, ProcessOpenFilesProvider,
    ProcessResourceControlProvider, ProcessResourcesProvider, ProcessThreadsProvider,
};
use taskmanager_platform_runtime::{
    ProcessControlExecutors, ProcessObservationExecutors, ProcessProviderBindings,
    ProcessProviderBindingsInput, ProviderRegistration,
};

/// macOS process provider composition grouped by scheduling responsibility.
pub struct MacProcessObservationProviders {
    list: ProviderRegistration<ProcessListRequest, Box<dyn ProcessListProvider>>,
    network: ProviderRegistration<ProcessNetworkRequest, Box<dyn ProcessNetworkProvider>>,
    gpu: ProviderRegistration<ProcessGpuRequest, Box<dyn ProcessGpuProvider>>,
    resources: ProviderRegistration<ProcessResourcesRequest, Box<dyn ProcessResourcesProvider>>,
    isolation: ProviderRegistration<ProcessIsolationRequest, Box<dyn ProcessIsolationProvider>>,
    threads: ProviderRegistration<ProcessThreadsRequest, Box<dyn ProcessThreadsProvider>>,
    affinity: ProviderRegistration<ProcessAffinityRequest, Box<dyn ProcessAffinityProvider>>,
    open_files:
        Option<ProviderRegistration<ProcessOpenFilesRequest, Box<dyn ProcessOpenFilesProvider>>>,
}

impl MacProcessObservationProviders {
    // The seven arguments are the seven independent process-observation
    // capabilities; grouping them would recreate the aggregate provider bag
    // this registry exists to prevent (same tradeoff as the platform
    // registries and the runtime bindings).
    #[must_use]
    pub fn new<L, N, G, R, I, T, A>(
        list: ProviderRegistration<ProcessListRequest, L>,
        network: ProviderRegistration<ProcessNetworkRequest, N>,
        gpu: ProviderRegistration<ProcessGpuRequest, G>,
        resources: ProviderRegistration<ProcessResourcesRequest, R>,
        isolation: ProviderRegistration<ProcessIsolationRequest, I>,
        threads: ProviderRegistration<ProcessThreadsRequest, T>,
        affinity: ProviderRegistration<ProcessAffinityRequest, A>,
    ) -> Self
    where
        L: ProcessListProvider,
        N: ProcessNetworkProvider,
        G: ProcessGpuProvider,
        R: ProcessResourcesProvider,
        I: ProcessIsolationProvider,
        T: ProcessThreadsProvider,
        A: ProcessAffinityProvider,
    {
        Self {
            list: list.map_provider(|provider| Box::new(provider) as Box<dyn ProcessListProvider>),
            network: network
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessNetworkProvider>),
            gpu: gpu.map_provider(|provider| Box::new(provider) as Box<dyn ProcessGpuProvider>),
            resources: resources
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessResourcesProvider>),
            isolation: isolation
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessIsolationProvider>),
            threads: threads
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessThreadsProvider>),
            affinity: affinity
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessAffinityProvider>),
            open_files: None,
        }
    }

    /// Attach the optional OpenFiles insight facet (the per-fd listing).
    /// Registered as a pending provider on macOS so the capability keeps an
    /// honest catalog descriptor typed `Unsupported` instead of being absent
    /// from enumeration (mirrors `with_desktop_notification` on Linux).
    #[must_use]
    pub fn with_open_files<O>(
        mut self,
        open_files: ProviderRegistration<ProcessOpenFilesRequest, O>,
    ) -> Self
    where
        O: ProcessOpenFilesProvider,
    {
        self.open_files = Some(
            open_files
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessOpenFilesProvider>),
        );
        self
    }

    pub(crate) fn into_runtime(self) -> ProcessObservationExecutors {
        let Self {
            list,
            network,
            gpu,
            resources,
            isolation,
            threads,
            affinity,
            open_files,
        } = self;
        let mut list = list.into_provider();
        let mut network = network.into_provider();
        let mut gpu = gpu.into_provider();
        let mut resources = resources.into_provider();
        let mut isolation = isolation.into_provider();
        let mut threads = threads.into_provider();
        let mut affinity = affinity.into_provider();
        let executors = ProcessObservationExecutors::new(
            move |observed_at_ms| list.refresh(observed_at_ms),
            move |target, observed_at_ms| network.observe(&target, observed_at_ms),
            move |target, observed_at_ms| gpu.observe(&target, observed_at_ms),
            move |target, observed_at_ms| resources.observe(&target, observed_at_ms),
            move |target, observed_at_ms| isolation.observe(&target, observed_at_ms),
            move |target, observed_at_ms| threads.observe(&target, observed_at_ms),
            move |target| affinity.affinity(&target),
        );
        match open_files {
            Some(open_files) => {
                let mut open_files = open_files.into_provider();
                executors.with_open_files(move |target, observed_at_ms| {
                    open_files.observe(&target, observed_at_ms)
                })
            }
            None => executors,
        }
    }
}

/// macOS process control providers.
pub struct MacProcessControlProviders {
    affinity_control: ProviderRegistration<
        ProcessAffinityControlRequest,
        Box<dyn ProcessAffinityControlProvider>,
    >,
    resource_control: ProviderRegistration<
        ProcessResourceControlRequest,
        Box<dyn ProcessResourceControlProvider>,
    >,
    network_escalation: ProviderRegistration<
        ProcessNetworkEscalationRequest,
        Box<dyn ProcessNetworkEscalationProvider>,
    >,
    control: ProviderRegistration<ProcessControlRequest, Box<dyn ProcessControlProvider>>,
}

impl MacProcessControlProviders {
    #[must_use]
    pub fn new<A, C, R, E>(
        affinity_control: ProviderRegistration<ProcessAffinityControlRequest, A>,
        control: ProviderRegistration<ProcessControlRequest, C>,
        resource_control: ProviderRegistration<ProcessResourceControlRequest, R>,
        network_escalation: ProviderRegistration<ProcessNetworkEscalationRequest, E>,
    ) -> Self
    where
        A: ProcessAffinityControlProvider,
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
            network_escalation: network_escalation.map_provider(|provider| {
                Box::new(provider) as Box<dyn ProcessNetworkEscalationProvider>
            }),
            control: control
                .map_provider(|provider| Box::new(provider) as Box<dyn ProcessControlProvider>),
        }
    }

    pub(crate) fn into_runtime(self) -> ProcessControlExecutors {
        use taskmanager_platform_runtime::ProcessControlCompletion;
        let Self {
            affinity_control,
            resource_control,
            network_escalation,
            control,
        } = self;
        let mut affinity_control = affinity_control.into_provider();
        let mut resource_control = resource_control.into_provider();
        let mut network_escalation = network_escalation.into_provider();
        let mut control = control.into_provider();
        ProcessControlExecutors::new(
            move |target, cpus| affinity_control.set_affinity(&target, &cpus),
            move |request| match request {
                taskmanager_application::ProcessControlRequest::EndTask(target) => {
                    control.end_task(target.clone())?;
                    Ok(ProcessControlCompletion::EndTask(target))
                }
                taskmanager_application::ProcessControlRequest::ExecuteBatch(intent) => Ok(
                    ProcessControlCompletion::Batch(control.execute_batch(intent)?),
                ),
                taskmanager_application::ProcessControlRequest::SendSignal { target, signal } => {
                    control.send_signal(&target, signal)?;
                    Ok(ProcessControlCompletion::Signal { target, signal })
                }
                // The neutral suspend/resume concepts map onto the sysinfo
                // stop/continue signals at this adapter edge — the same
                // `signal_pid` primitive the batch arms use — and completion
                // rides the signal event.
                taskmanager_application::ProcessControlRequest::Suspend { target } => {
                    control.send_signal(&target, ProcessSignal::Stop)?;
                    Ok(ProcessControlCompletion::Signal {
                        target,
                        signal: ProcessSignal::Stop,
                    })
                }
                taskmanager_application::ProcessControlRequest::Resume { target } => {
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

/// macOS process provider composition.
pub struct MacProcessProviders {
    observations: MacProcessObservationProviders,
    controls: MacProcessControlProviders,
}

impl MacProcessProviders {
    #[must_use]
    pub const fn new(
        observations: MacProcessObservationProviders,
        controls: MacProcessControlProviders,
    ) -> Self {
        Self {
            observations,
            controls,
        }
    }

    pub(crate) fn runtime_bindings(&self) -> ProcessProviderBindings {
        let bindings = ProcessProviderBindings::new(ProcessProviderBindingsInput {
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
        });
        match &self.observations.open_files {
            Some(open_files) => bindings.with_open_files(open_files),
            None => bindings,
        }
    }

    pub(crate) fn into_runtime(self) -> taskmanager_platform_runtime::ProcessExecutors {
        taskmanager_platform_runtime::ProcessExecutors::new(
            self.observations.into_runtime(),
            self.controls.into_runtime(),
        )
    }
}
