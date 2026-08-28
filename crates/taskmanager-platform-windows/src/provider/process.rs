//! Windows process-domain providers built on safe wrapper crates plus the
//! audited exact-process boundary for destructive control.
//!
//! List/resources use `sysinfo` (SystemProcessInformation behind a safe API);
//! per-process GPU facts come from the WDDM PDH counters (Task Manager's own
//! source) with `nvml-wrapper` as the fallback; thread details come from the
//! audited ToolHelp32/GetThreadTimes boundary; suspend/resume use the audited
//! per-thread `SuspendThread`/`ResumeThread` boundary. The per-fd open-files
//! insight walks the audited system-handle-table boundary (route B, ADR-018):
//! only File-type objects are reported, and sockets stay to the connections
//! insight, which owns that fact on Windows.

use taskmanager_application::{
    ProcessAffinityControlRequest, ProcessAffinityRequest, ProcessControlRequest,
    ProcessEnvironmentRequest, ProcessGpuRequest, ProcessIsolationRequest, ProcessListRequest,
    ProcessNetworkEscalationRequest, ProcessNetworkRequest, ProcessOpenFilesRequest,
    ProcessResourceControlRequest, ProcessResourcesRequest, ProcessThreadsRequest,
};
use taskmanager_core::{
    DeviceState, FailureKind, FrozenProcessIdentity, PriorityTier, ProcessBatchAction,
    ProcessBatchIntent, ProcessBatchResult, ProcessBatchTargetResult, ProcessGpuDevice,
    ProcessGpuEngines, ProcessGpuSnapshot, ProcessIdentity, ProcessInsightSnapshot,
    ProcessResourceObservations, ProcessResourceSnapshot, ProcessSignal, ProviderId,
    ResourceObservation,
};
use taskmanager_platform_contract::{ProviderFailure, SourceOutcome, SourceStatus};
use taskmanager_platform_provider::{
    ProcessAffinityControlProvider, ProcessAffinityProvider, ProcessControlProvider,
    ProcessEnvironmentProvider, ProcessGpuProvider, ProcessIsolationProvider, ProcessListProvider,
    ProcessNetworkEscalationProvider, ProcessNetworkProvider, ProcessOpenFilesProvider,
    ProcessResourceControlProvider, ProcessResourcesProvider, ProcessThreadsProvider,
};
use taskmanager_platform_runtime::{
    ProcessControlExecutors, ProcessObservationExecutors, ProcessProviderBindings,
    ProcessProviderBindingsInput, ProviderRegistration,
};

mod control;
mod insights;
mod list;
pub(crate) mod target_observation;

pub use control::*;
pub use insights::{
    WinProcessEnvironmentProvider, WinProcessOpenFilesProvider, WinProcessThreadsProvider,
};
pub use list::WinProcessListProvider;

const PROCESS_LIST_PROVIDER: ProviderId = ProviderId::borrowed("windows.process.list.sysinfo");
const PROCESS_RESOURCE_MEMORY_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.resources.sysinfo");
const PROCESS_RESOURCE_LIMITS_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.process.resources.job-object");

fn source(provider: ProviderId, item_count: usize) -> SourceStatus {
    SourceStatus {
        provider,
        outcome: SourceOutcome::Available,
        item_count,
    }
}

fn unsupported_resource<T>() -> ResourceObservation<T> {
    ResourceObservation::unavailable(FailureKind::Unsupported)
}

fn resource_snapshot(memory_usage_bytes: u64, observed_at_ms: u64) -> ProcessResourceSnapshot {
    ProcessResourceSnapshot::from_observations(
        DeviceState::healthy(observed_at_ms),
        ProcessResourceObservations {
            limits: unsupported_resource(),
            resource_groups: unsupported_resource(),
            memory_usage_bytes: ResourceObservation::current(memory_usage_bytes, observed_at_ms),
            memory_limit: unsupported_resource(),
            cpu_time_quota_micros: unsupported_resource(),
            cpu_time_period_micros: unsupported_resource(),
            process_count: unsupported_resource(),
            process_limit: unsupported_resource(),
        },
        vec![
            source(PROCESS_RESOURCE_MEMORY_PROVIDER, 1),
            SourceStatus {
                provider: PROCESS_RESOURCE_LIMITS_PROVIDER,
                outcome: SourceOutcome::Unavailable(FailureKind::Unsupported),
                item_count: 0,
            },
        ],
    )
}

/// Resolve and verify the provider-issued process identity before any
/// target-scoped read. A PID is only a locator; the creation-time token is
/// the authority. Callers that perform a multi-step native query validate a
/// second time after the read so PID reuse cannot publish replacement facts.
pub(crate) fn validate_process_target(
    target: &FrozenProcessIdentity,
) -> Result<u64, ProviderFailure> {
    let expected = target
        .authoritative_start_token()
        .ok_or(ProviderFailure::IdentityChanged)?;
    #[cfg(windows)]
    {
        let actual = taskmanager_windows_api::process_creation_time_100ns(target.pid)
            .map_err(map_windows_api_failure)?;
        if actual != expected {
            return Err(ProviderFailure::IdentityChanged);
        }
        Ok(expected)
    }
    #[cfg(not(windows))]
    {
        let _ = expected;
        Err(ProviderFailure::Unsupported)
    }
}

fn validate_process_target_after(
    target: &FrozenProcessIdentity,
    expected: u64,
) -> Result<(), ProviderFailure> {
    #[cfg(windows)]
    {
        let actual = taskmanager_windows_api::process_creation_time_100ns(target.pid)
            .map_err(map_windows_api_failure)?;
        if actual != expected {
            return Err(ProviderFailure::IdentityChanged);
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (target, expected);
        Err(ProviderFailure::Unsupported)
    }
}

fn snapshot_identity(target: &FrozenProcessIdentity) -> ProcessIdentity {
    ProcessIdentity {
        pid: target.pid,
        start_token: target.authoritative_start_token().unwrap_or(0),
    }
}

/// Windows process provider composition grouped by scheduling responsibility.
pub struct WinProcessObservationProviders {
    pub(crate) list: ProviderRegistration<ProcessListRequest, Box<dyn ProcessListProvider>>,
    pub(crate) network:
        ProviderRegistration<ProcessNetworkRequest, Box<dyn ProcessNetworkProvider>>,
    pub(crate) gpu: ProviderRegistration<ProcessGpuRequest, Box<dyn ProcessGpuProvider>>,
    pub(crate) resources:
        ProviderRegistration<ProcessResourcesRequest, Box<dyn ProcessResourcesProvider>>,
    pub(crate) isolation:
        ProviderRegistration<ProcessIsolationRequest, Box<dyn ProcessIsolationProvider>>,
    pub(crate) threads:
        ProviderRegistration<ProcessThreadsRequest, Box<dyn ProcessThreadsProvider>>,
    pub(crate) affinity:
        ProviderRegistration<ProcessAffinityRequest, Box<dyn ProcessAffinityProvider>>,
    pub(crate) open_files:
        Option<ProviderRegistration<ProcessOpenFilesRequest, Box<dyn ProcessOpenFilesProvider>>>,
    pub(crate) environment: Option<
        ProviderRegistration<ProcessEnvironmentRequest, Box<dyn ProcessEnvironmentProvider>>,
    >,
}

impl WinProcessObservationProviders {
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
            environment: None,
        }
    }

    /// Attach the optional OpenFiles insight facet (the per-fd listing).
    /// Registered as a pending provider on Windows so the capability keeps an
    /// honest catalog descriptor typed `Unsupported` instead of being absent
    /// from enumeration (mirrors `with_open_files` on the Linux/macOS
    /// adapters).
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

    /// Attach the optional environment insight facet (variables + working
    /// directory). Wired through the runtime's environment facet like the
    /// Linux adapter's `with_environment` composition.
    #[must_use]
    pub fn with_environment<E>(
        mut self,
        environment: ProviderRegistration<ProcessEnvironmentRequest, E>,
    ) -> Self
    where
        E: ProcessEnvironmentProvider,
    {
        self.environment =
            Some(environment.map_provider(|provider| {
                Box::new(provider) as Box<dyn ProcessEnvironmentProvider>
            }));
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
            environment,
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
        let executors = match open_files {
            Some(open_files) => {
                let mut open_files = open_files.into_provider();
                executors.with_open_files(move |target, observed_at_ms| {
                    open_files.observe(&target, observed_at_ms)
                })
            }
            None => executors,
        };
        match environment {
            Some(environment) => {
                let mut environment = environment.into_provider();
                executors.with_environment(move |target, observed_at_ms| {
                    environment.observe(&target, observed_at_ms)
                })
            }
            None => executors,
        }
    }
}

/// Windows process provider composition.
pub struct WinProcessProviders {
    observations: WinProcessObservationProviders,
    controls: WinProcessControlProviders,
}

impl WinProcessProviders {
    #[must_use]
    pub const fn new(
        observations: WinProcessObservationProviders,
        controls: WinProcessControlProviders,
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
        let bindings = match &self.observations.open_files {
            Some(open_files) => bindings.with_open_files(open_files),
            None => bindings,
        };
        match &self.observations.environment {
            Some(environment) => bindings.with_environment(environment),
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

#[cfg(test)]
#[path = "../../tests/headless/provider/process.rs"]
mod tests;
