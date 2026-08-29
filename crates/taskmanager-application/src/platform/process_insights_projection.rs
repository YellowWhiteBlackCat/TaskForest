//! Application-owned merge policy for independently scheduled process insights.

use taskmanager_core::{
    FailureKind, ProcessEnvironment, ProcessGpuSnapshot, ProcessIdentity, ProcessIsolation,
    ProcessNetworkSnapshot, ProcessOpenFiles, ProcessResourceSnapshot, ProcessTelemetrySnapshot,
    ProcessThreads,
};
use taskmanager_platform_contract::SubmissionErrorKind;

use super::{ProcessInsightFacetEvent, ProcessInsightObservation};
use taskmanager_core::core::process::FrozenProcessIdentity;

use crate::ProcessInsightsRevision;

mod terminal;
use terminal::{
    aggregate_usable_state, terminal_environment, terminal_gpu, terminal_isolation,
    terminal_network, terminal_open_files, terminal_resources, terminal_threads,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessInsightFacet {
    Network,
    Gpu,
    Resources,
    Isolation,
    Threads,
    OpenFiles,
    Environment,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum ProcessInsightFacetState<T> {
    #[default]
    Pending,
    Current(T),
    Unavailable(ProcessInsightUnavailable),
}

/// Typed reason why one independently scheduled facet cannot contribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessInsightUnavailable {
    Provider(FailureKind),
    Submission(SubmissionErrorKind),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedProcessInsights {
    pub target: FrozenProcessIdentity,
    pub revision: ProcessInsightsRevision,
    pub network: ProcessInsightFacetState<ProcessNetworkSnapshot>,
    pub gpu: ProcessInsightFacetState<ProcessGpuSnapshot>,
    pub resources: ProcessInsightFacetState<ProcessResourceSnapshot>,
    pub isolation: ProcessInsightFacetState<ProcessIsolation>,
    pub threads: ProcessInsightFacetState<ProcessThreads>,
    pub open_files: ProcessInsightFacetState<ProcessOpenFiles>,
    pub environment: ProcessInsightFacetState<ProcessEnvironment>,
    raw_identity: Option<ProcessIdentity>,
}

impl ProjectedProcessInsights {
    pub(crate) fn pending(
        target: FrozenProcessIdentity,
        revision: ProcessInsightsRevision,
    ) -> Self {
        Self {
            target,
            revision,
            network: ProcessInsightFacetState::Pending,
            gpu: ProcessInsightFacetState::Pending,
            resources: ProcessInsightFacetState::Pending,
            isolation: ProcessInsightFacetState::Pending,
            threads: ProcessInsightFacetState::Pending,
            open_files: ProcessInsightFacetState::Pending,
            environment: ProcessInsightFacetState::Pending,
            raw_identity: None,
        }
    }

    #[must_use]
    pub const fn raw_identity(&self) -> Option<ProcessIdentity> {
        self.raw_identity
    }

    #[must_use]
    pub fn complete_snapshot(&self) -> Option<ProcessTelemetrySnapshot> {
        let (network, network_current) = terminal_network(&self.network)?;
        let (gpu, gpu_current) = terminal_gpu(&self.gpu)?;
        let (resources, resources_current) = terminal_resources(&self.resources)?;
        let (isolation, isolation_current) = terminal_isolation(&self.isolation)?;
        let (threads, threads_current) = terminal_threads(&self.threads);
        let (open_files, open_files_current) = terminal_open_files(&self.open_files);
        let (environment, environment_current) = terminal_environment(&self.environment);
        let mut current_states = Vec::new();
        if network_current {
            current_states.extend([network.state, network.traffic_state]);
        }
        if gpu_current {
            current_states.push(gpu.state);
        }
        if resources_current {
            current_states.push(resources.state());
        }
        if isolation_current {
            current_states.push(isolation.state);
        }
        if threads_current {
            current_states.push(threads.state);
        }
        if open_files_current {
            current_states.push(open_files.state);
        }
        if environment_current {
            current_states.push(environment.state);
        }
        if current_states.is_empty() {
            return None;
        }
        let identity = self.raw_identity?;
        let state = aggregate_usable_state(current_states);
        Some(ProcessTelemetrySnapshot {
            identity,
            state,
            network,
            gpu,
            resources,
            isolation,
            // Threads and open files are optional non-blocking enrichment: a
            // pending first response remains typed Unsupported until its own
            // bounded lane publishes.
            open_files,
            threads,
            environment,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessInsightsProjectionRejection {
    NoActiveRequest,
    StaleOrUnexpectedRevision,
    DifferentFrozenTarget,
    ConflictingRawIdentity,
    DuplicateFacet,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProcessInsightsProjectionApplyResult {
    AppliedPartial(Box<ProjectedProcessInsights>),
    AppliedComplete {
        projection: Box<ProjectedProcessInsights>,
        complete_snapshot: Box<ProcessTelemetrySnapshot>,
    },
    Ignored(ProcessInsightsProjectionRejection),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcessInsightsProjection {
    current: Option<ProjectedProcessInsights>,
}

impl ProcessInsightsProjection {
    /// Begin one application-selected target/revision. This explicit selection
    /// prevents a late event for another PID generation from replacing state.
    pub fn begin(&mut self, target: FrozenProcessIdentity, revision: ProcessInsightsRevision) {
        let retain_current = self
            .current
            .as_ref()
            .is_some_and(|current| current.target == target && current.revision >= revision);
        if !retain_current {
            self.current = Some(ProjectedProcessInsights::pending(target, revision));
        }
    }

    #[must_use]
    pub fn current(&self) -> Option<&ProjectedProcessInsights> {
        self.current.as_ref()
    }

    pub fn apply(
        &mut self,
        event: &ProcessInsightFacetEvent,
    ) -> ProcessInsightsProjectionApplyResult {
        match event {
            ProcessInsightFacetEvent::Network(observation) => self.apply_network(observation),
            ProcessInsightFacetEvent::Gpu(observation) => self.apply_gpu(observation),
            ProcessInsightFacetEvent::Resources(observation) => self.apply_resources(observation),
            ProcessInsightFacetEvent::Isolation(observation) => self.apply_isolation(observation),
            ProcessInsightFacetEvent::Threads(observation) => self.apply_threads(observation),
            ProcessInsightFacetEvent::OpenFiles(observation) => self.apply_open_files(observation),
            ProcessInsightFacetEvent::Environment(observation) => {
                self.apply_environment(observation)
            }
        }
    }

    pub fn apply_failure(
        &mut self,
        target: &FrozenProcessIdentity,
        revision: ProcessInsightsRevision,
        facet: ProcessInsightFacet,
        failure: ProcessInsightUnavailable,
    ) -> ProcessInsightsProjectionApplyResult {
        let Some(current) = self.current.as_mut() else {
            return ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::NoActiveRequest,
            );
        };
        if current.target != *target {
            return ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::DifferentFrozenTarget,
            );
        }
        if current.revision != revision {
            return ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::StaleOrUnexpectedRevision,
            );
        }
        match facet {
            ProcessInsightFacet::Network => {
                current.network = ProcessInsightFacetState::Unavailable(failure);
            }
            ProcessInsightFacet::Gpu => {
                current.gpu = ProcessInsightFacetState::Unavailable(failure);
            }
            ProcessInsightFacet::Resources => {
                current.resources = ProcessInsightFacetState::Unavailable(failure);
            }
            ProcessInsightFacet::Isolation => {
                current.isolation = ProcessInsightFacetState::Unavailable(failure);
            }
            ProcessInsightFacet::Threads => {
                current.threads = ProcessInsightFacetState::Unavailable(failure);
            }
            ProcessInsightFacet::OpenFiles => {
                current.open_files = ProcessInsightFacetState::Unavailable(failure);
            }
            ProcessInsightFacet::Environment => {
                current.environment = ProcessInsightFacetState::Unavailable(failure);
            }
        }
        Self::applied(current)
    }

    /// Snapshot the application-owned partial state after synchronous bounded
    /// submissions have marked absent/busy facets unavailable.
    #[must_use]
    pub fn snapshot(&self) -> Option<ProjectedProcessInsights> {
        self.current.clone()
    }

    fn apply_network(
        &mut self,
        observation: &ProcessInsightObservation<ProcessNetworkSnapshot>,
    ) -> ProcessInsightsProjectionApplyResult {
        if let Err(result) = self.prepare(observation, ProcessInsightFacet::Network) {
            return result;
        }
        let Some(current) = self.current.as_mut() else {
            return ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::NoActiveRequest,
            );
        };
        current.network = ProcessInsightFacetState::Current(observation.snapshot.value.clone());
        Self::applied(current)
    }

    fn apply_gpu(
        &mut self,
        observation: &ProcessInsightObservation<ProcessGpuSnapshot>,
    ) -> ProcessInsightsProjectionApplyResult {
        if let Err(result) = self.prepare(observation, ProcessInsightFacet::Gpu) {
            return result;
        }
        let Some(current) = self.current.as_mut() else {
            return ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::NoActiveRequest,
            );
        };
        current.gpu = ProcessInsightFacetState::Current(observation.snapshot.value.clone());
        Self::applied(current)
    }

    fn apply_resources(
        &mut self,
        observation: &ProcessInsightObservation<ProcessResourceSnapshot>,
    ) -> ProcessInsightsProjectionApplyResult {
        if let Err(result) = self.prepare(observation, ProcessInsightFacet::Resources) {
            return result;
        }
        let Some(current) = self.current.as_mut() else {
            return ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::NoActiveRequest,
            );
        };
        current.resources = ProcessInsightFacetState::Current(observation.snapshot.value.clone());
        Self::applied(current)
    }

    fn apply_isolation(
        &mut self,
        observation: &ProcessInsightObservation<ProcessIsolation>,
    ) -> ProcessInsightsProjectionApplyResult {
        if let Err(result) = self.prepare(observation, ProcessInsightFacet::Isolation) {
            return result;
        }
        let Some(current) = self.current.as_mut() else {
            return ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::NoActiveRequest,
            );
        };
        current.isolation = ProcessInsightFacetState::Current(observation.snapshot.value.clone());
        Self::applied(current)
    }

    fn apply_threads(
        &mut self,
        observation: &ProcessInsightObservation<ProcessThreads>,
    ) -> ProcessInsightsProjectionApplyResult {
        if let Err(result) = self.prepare(observation, ProcessInsightFacet::Threads) {
            return result;
        }
        let Some(current) = self.current.as_mut() else {
            return ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::NoActiveRequest,
            );
        };
        current.threads = ProcessInsightFacetState::Current(observation.snapshot.value.clone());
        Self::applied(current)
    }

    fn apply_open_files(
        &mut self,
        observation: &ProcessInsightObservation<ProcessOpenFiles>,
    ) -> ProcessInsightsProjectionApplyResult {
        if let Err(result) = self.prepare(observation, ProcessInsightFacet::OpenFiles) {
            return result;
        }
        let Some(current) = self.current.as_mut() else {
            return ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::NoActiveRequest,
            );
        };
        current.open_files = ProcessInsightFacetState::Current(observation.snapshot.value.clone());
        Self::applied(current)
    }

    fn apply_environment(
        &mut self,
        observation: &ProcessInsightObservation<ProcessEnvironment>,
    ) -> ProcessInsightsProjectionApplyResult {
        if let Err(result) = self.prepare(observation, ProcessInsightFacet::Environment) {
            return result;
        }
        let Some(current) = self.current.as_mut() else {
            return ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::NoActiveRequest,
            );
        };
        current.environment = ProcessInsightFacetState::Current(observation.snapshot.value.clone());
        Self::applied(current)
    }

    fn prepare<T>(
        &mut self,
        observation: &ProcessInsightObservation<T>,
        facet: ProcessInsightFacet,
    ) -> Result<(), ProcessInsightsProjectionApplyResult> {
        let Some(current) = self.current.as_mut() else {
            return Err(ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::NoActiveRequest,
            ));
        };
        if current.target != observation.target {
            return Err(ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::DifferentFrozenTarget,
            ));
        }
        if current.revision != observation.revision {
            return Err(ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::StaleOrUnexpectedRevision,
            ));
        }
        if current
            .raw_identity
            .is_some_and(|identity| identity != observation.snapshot.identity)
        {
            return Err(ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::ConflictingRawIdentity,
            ));
        }
        if facet_is_set(current, facet) {
            return Err(ProcessInsightsProjectionApplyResult::Ignored(
                ProcessInsightsProjectionRejection::DuplicateFacet,
            ));
        }
        current.raw_identity = Some(observation.snapshot.identity);
        Ok(())
    }

    fn applied(current: &ProjectedProcessInsights) -> ProcessInsightsProjectionApplyResult {
        match current.complete_snapshot() {
            Some(complete_snapshot) => ProcessInsightsProjectionApplyResult::AppliedComplete {
                projection: Box::new(current.clone()),
                complete_snapshot: Box::new(complete_snapshot),
            },
            None => ProcessInsightsProjectionApplyResult::AppliedPartial(Box::new(current.clone())),
        }
    }
}

fn facet_is_set(current: &ProjectedProcessInsights, facet: ProcessInsightFacet) -> bool {
    match facet {
        ProcessInsightFacet::Network => {
            !matches!(current.network, ProcessInsightFacetState::Pending)
        }
        ProcessInsightFacet::Gpu => !matches!(current.gpu, ProcessInsightFacetState::Pending),
        ProcessInsightFacet::Resources => {
            !matches!(current.resources, ProcessInsightFacetState::Pending)
        }
        ProcessInsightFacet::Isolation => {
            !matches!(current.isolation, ProcessInsightFacetState::Pending)
        }
        ProcessInsightFacet::Threads => {
            !matches!(current.threads, ProcessInsightFacetState::Pending)
        }
        ProcessInsightFacet::OpenFiles => {
            !matches!(current.open_files, ProcessInsightFacetState::Pending)
        }
        ProcessInsightFacet::Environment => {
            !matches!(current.environment, ProcessInsightFacetState::Pending)
        }
    }
}

#[cfg(test)]
#[path = "../../tests/headless/application_platform_process_insights_projection_tests.rs"]
mod tests;
