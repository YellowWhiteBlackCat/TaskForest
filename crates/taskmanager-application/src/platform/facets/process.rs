//! Process capability ports and events.
//!
//! Defines process list, control, the independently scheduled insight facets, affinity, and
//! resource-control requests plus the `ProcessFacets` group of independently
//! optional ports.

use std::sync::Arc;

use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchIntent, ProcessBatchResult, ProcessItem, ProcessSignal,
};
use taskmanager_core::core::process_telemetry::{
    ProcessEnvironment, ProcessGpuSnapshot, ProcessInsightSnapshot, ProcessIsolation,
    ProcessNetworkSnapshot, ProcessOpenFiles, ProcessResourceSnapshot, ProcessThreads,
    ResourceGroupLimitRequest,
};
use taskmanager_platform_contract::{
    CapabilityId, CapabilityRequest, RequestId, RequestPort, RequestScope, RequestTracking,
    RequestTrackingError, SubmissionError,
};

#[derive(Clone, Debug)]
pub enum ProcessEvent {
    Snapshot(Vec<ProcessItem>),
    EndTaskCompleted(FrozenProcessIdentity),
    BatchCompleted(ProcessBatchResult),
    SignalCompleted {
        target: FrozenProcessIdentity,
        signal: ProcessSignal,
    },
    AffinityApplied {
        target: FrozenProcessIdentity,
        cpus: Vec<u32>,
    },
    ResourceLimitsApplied {
        target: FrozenProcessIdentity,
        limits: ResourceGroupLimitRequest,
    },
    NetworkCaptureEscalated,
}

impl ProcessEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        match self {
            Self::Snapshot(_) => capability == &CapabilityId::PROCESS_LIST,
            Self::EndTaskCompleted(_) | Self::BatchCompleted(_) | Self::SignalCompleted { .. } => {
                capability == &CapabilityId::PROCESS_CONTROL
            }
            Self::AffinityApplied { .. } => capability == &CapabilityId::PROCESS_AFFINITY_CONTROL,
            Self::ResourceLimitsApplied { .. } => {
                capability == &CapabilityId::PROCESS_RESOURCE_CONTROL
            }
            Self::NetworkCaptureEscalated => {
                capability == &CapabilityId::PROCESS_NETWORK_ESCALATION
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessListRequest {
    Refresh,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessControlRequest {
    EndTask(FrozenProcessIdentity),
    ExecuteBatch(ProcessBatchIntent),
    SendSignal {
        target: FrozenProcessIdentity,
        signal: ProcessSignal,
    },
    /// Semantically suspend (freeze) one process. Adapters map the concept to
    /// the native primitive (Linux: SIGSTOP; macOS: stop signal); a platform
    /// without a safe primitive completes with a typed failure (ARCH §8.1
    /// 映射穷尽律). UIs must prefer this over `SendSignal(Stop)` — signal
    /// vocabulary is an adapter mapping detail, not the user concept.
    Suspend {
        target: FrozenProcessIdentity,
    },
    /// Semantically resume one previously suspended process (Linux: SIGCONT).
    Resume {
        target: FrozenProcessIdentity,
    },
}

bind_request_capability!(ProcessListRequest, CapabilityId::PROCESS_LIST);

impl CapabilityRequest for ProcessControlRequest {
    const CAPABILITY: CapabilityId = CapabilityId::PROCESS_CONTROL;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        match self {
            Self::EndTask(target)
            | Self::SendSignal { target, .. }
            | Self::Suspend { target }
            | Self::Resume { target } => process_target_tracking(target),
            // A batch is one transaction over a frozen set. Its terminal
            // lifecycle must not be split into unrelated per-target jobs.
            Self::ExecuteBatch(_) => Ok(RequestTracking::Capability),
        }
    }
}

/// Application-owned generation correlating independently scheduled process
/// insights for one frozen target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessInsightsRevision(u64);

impl ProcessInsightsRevision {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(next) => Some(Self(next)),
            None => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessNetworkRequest {
    pub target: FrozenProcessIdentity,
    pub revision: ProcessInsightsRevision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessGpuRequest {
    pub target: FrozenProcessIdentity,
    pub revision: ProcessInsightsRevision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessResourcesRequest {
    pub target: FrozenProcessIdentity,
    pub revision: ProcessInsightsRevision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessIsolationRequest {
    pub target: FrozenProcessIdentity,
    pub revision: ProcessInsightsRevision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessThreadsRequest {
    pub target: FrozenProcessIdentity,
    pub revision: ProcessInsightsRevision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessOpenFilesRequest {
    pub target: FrozenProcessIdentity,
    pub revision: ProcessInsightsRevision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessEnvironmentRequest {
    pub target: FrozenProcessIdentity,
    pub revision: ProcessInsightsRevision,
}

macro_rules! bind_process_target_request {
    ($request:ty, $capability:expr) => {
        impl CapabilityRequest for $request {
            const CAPABILITY: CapabilityId = $capability;

            fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
                process_target_tracking(&self.target)
            }
        }
    };
}

bind_process_target_request!(
    ProcessNetworkRequest,
    CapabilityId::PROCESS_INSIGHTS_NETWORK
);
bind_process_target_request!(ProcessGpuRequest, CapabilityId::PROCESS_INSIGHTS_GPU);
bind_process_target_request!(
    ProcessResourcesRequest,
    CapabilityId::PROCESS_INSIGHTS_RESOURCES
);
bind_process_target_request!(
    ProcessIsolationRequest,
    CapabilityId::PROCESS_INSIGHTS_ISOLATION
);
bind_process_target_request!(
    ProcessThreadsRequest,
    CapabilityId::PROCESS_INSIGHTS_THREADS
);
bind_process_target_request!(
    ProcessOpenFilesRequest,
    CapabilityId::PROCESS_INSIGHTS_OPEN_FILES
);
bind_process_target_request!(
    ProcessEnvironmentRequest,
    CapabilityId::PROCESS_INSIGHTS_ENVIRONMENT
);

/// Honest result of the independent bounded submissions made for one
/// application-selected process generation.
///
/// A successful facet can continue even when another queue is busy or absent;
/// callers must not interpret this value as an all-or-nothing aggregate.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessInsightsSubmission {
    pub target: FrozenProcessIdentity,
    pub revision: ProcessInsightsRevision,
    pub network: Result<RequestId, SubmissionError>,
    pub gpu: Result<RequestId, SubmissionError>,
    pub resources: Result<RequestId, SubmissionError>,
    pub isolation: Result<RequestId, SubmissionError>,
    pub threads: Result<RequestId, SubmissionError>,
    pub open_files: Result<RequestId, SubmissionError>,
    pub environment: Result<RequestId, SubmissionError>,
    pub projection: super::super::ProjectedProcessInsights,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessInsightsSubmissionError {
    IdentityUnavailable,
    RevisionExhausted,
}

impl ProcessInsightsSubmission {
    #[must_use]
    pub fn has_pending_requests(&self) -> bool {
        self.network.is_ok()
            || self.gpu.is_ok()
            || self.resources.is_ok()
            || self.isolation.is_ok()
            || self.threads.is_ok()
            || self.open_files.is_ok()
            || self.environment.is_ok()
    }

    #[must_use]
    pub fn first_error(&self) -> Option<&SubmissionError> {
        [
            self.network.as_ref().err(),
            self.gpu.as_ref().err(),
            self.resources.as_ref().err(),
            self.isolation.as_ref().err(),
            self.threads.as_ref().err(),
            self.open_files.as_ref().err(),
            self.environment.as_ref().err(),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// One domain completion retaining both the application-facing frozen target
/// and the provider-native raw identity proof.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessInsightObservation<T> {
    pub target: FrozenProcessIdentity,
    pub revision: ProcessInsightsRevision,
    pub snapshot: ProcessInsightSnapshot<T>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProcessInsightFacetEvent {
    Network(Box<ProcessInsightObservation<ProcessNetworkSnapshot>>),
    Gpu(Box<ProcessInsightObservation<ProcessGpuSnapshot>>),
    Resources(Box<ProcessInsightObservation<ProcessResourceSnapshot>>),
    Isolation(Box<ProcessInsightObservation<ProcessIsolation>>),
    Threads(Box<ProcessInsightObservation<ProcessThreads>>),
    OpenFiles(Box<ProcessInsightObservation<ProcessOpenFiles>>),
    Environment(Box<ProcessInsightObservation<ProcessEnvironment>>),
}

impl ProcessInsightFacetEvent {
    #[must_use]
    pub const fn facet(&self) -> super::super::ProcessInsightFacet {
        match self {
            Self::Network(_) => super::super::ProcessInsightFacet::Network,
            Self::Gpu(_) => super::super::ProcessInsightFacet::Gpu,
            Self::Resources(_) => super::super::ProcessInsightFacet::Resources,
            Self::Isolation(_) => super::super::ProcessInsightFacet::Isolation,
            Self::Threads(_) => super::super::ProcessInsightFacet::Threads,
            Self::OpenFiles(_) => super::super::ProcessInsightFacet::OpenFiles,
            Self::Environment(_) => super::super::ProcessInsightFacet::Environment,
        }
    }

    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        match self {
            Self::Network(_) => capability == &CapabilityId::PROCESS_INSIGHTS_NETWORK,
            Self::Gpu(_) => capability == &CapabilityId::PROCESS_INSIGHTS_GPU,
            Self::Resources(_) => capability == &CapabilityId::PROCESS_INSIGHTS_RESOURCES,
            Self::Isolation(_) => capability == &CapabilityId::PROCESS_INSIGHTS_ISOLATION,
            Self::Threads(_) => capability == &CapabilityId::PROCESS_INSIGHTS_THREADS,
            Self::OpenFiles(_) => capability == &CapabilityId::PROCESS_INSIGHTS_OPEN_FILES,
            Self::Environment(_) => capability == &CapabilityId::PROCESS_INSIGHTS_ENVIRONMENT,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessAffinityRequest {
    pub target: FrozenProcessIdentity,
}

impl CapabilityRequest for ProcessAffinityRequest {
    const CAPABILITY: CapabilityId = CapabilityId::PROCESS_AFFINITY;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        process_target_tracking(&self.target)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessAffinityEvent {
    Snapshot {
        target: FrozenProcessIdentity,
        cpus: Vec<u32>,
    },
}

impl ProcessAffinityEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::PROCESS_AFFINITY
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessAffinityControlRequest {
    pub target: FrozenProcessIdentity,
    pub cpus: Vec<u32>,
}

impl CapabilityRequest for ProcessAffinityControlRequest {
    const CAPABILITY: CapabilityId = CapabilityId::PROCESS_AFFINITY_CONTROL;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        process_target_tracking(&self.target)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessResourceControlRequest {
    pub target: FrozenProcessIdentity,
    pub limits: ResourceGroupLimitRequest,
}

impl CapabilityRequest for ProcessResourceControlRequest {
    const CAPABILITY: CapabilityId = CapabilityId::PROCESS_RESOURCE_CONTROL;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        process_target_tracking(&self.target)
    }
}

fn process_target_tracking(
    target: &FrozenProcessIdentity,
) -> Result<RequestTracking, RequestTrackingError> {
    let Some(start_token) = target.authoritative_start_token() else {
        return Err(RequestTrackingError::MissingTargetIdentity);
    };
    RequestScope::try_owned(format!("{}:{start_token}", target.pid)).map(RequestTracking::Target)
}

/// System-level (no target) per-feature escalation for per-process byte
/// accounting: offer the OS-native prompt, consume the granted capture fd, and
/// restart the accounting backend with real capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessNetworkEscalationRequest;

bind_request_capability!(
    ProcessNetworkEscalationRequest,
    CapabilityId::PROCESS_NETWORK_ESCALATION
);

pub type ProcessListRequestPort = dyn RequestPort<Request = ProcessListRequest>;
pub type ProcessControlRequestPort = dyn RequestPort<Request = ProcessControlRequest>;
pub type ProcessNetworkRequestPort = dyn RequestPort<Request = ProcessNetworkRequest>;
pub type ProcessGpuRequestPort = dyn RequestPort<Request = ProcessGpuRequest>;
pub type ProcessResourcesRequestPort = dyn RequestPort<Request = ProcessResourcesRequest>;
pub type ProcessIsolationRequestPort = dyn RequestPort<Request = ProcessIsolationRequest>;
pub type ProcessThreadsRequestPort = dyn RequestPort<Request = ProcessThreadsRequest>;
pub type ProcessOpenFilesRequestPort = dyn RequestPort<Request = ProcessOpenFilesRequest>;
pub type ProcessEnvironmentRequestPort = dyn RequestPort<Request = ProcessEnvironmentRequest>;
pub type ProcessAffinityRequestPort = dyn RequestPort<Request = ProcessAffinityRequest>;
pub type ProcessAffinityControlRequestPort =
    dyn RequestPort<Request = ProcessAffinityControlRequest>;
pub type ProcessResourceControlRequestPort =
    dyn RequestPort<Request = ProcessResourceControlRequest>;
pub type ProcessNetworkEscalationRequestPort =
    dyn RequestPort<Request = ProcessNetworkEscalationRequest>;

/// Independently optional process ports. The group is a construction value,
/// not an aggregate request queue or provider.
#[derive(Clone, Default)]
pub struct ProcessFacets {
    list: Option<Arc<ProcessListRequestPort>>,
    control: Option<Arc<ProcessControlRequestPort>>,
    network: Option<Arc<ProcessNetworkRequestPort>>,
    gpu: Option<Arc<ProcessGpuRequestPort>>,
    resources: Option<Arc<ProcessResourcesRequestPort>>,
    isolation: Option<Arc<ProcessIsolationRequestPort>>,
    threads: Option<Arc<ProcessThreadsRequestPort>>,
    open_files: Option<Arc<ProcessOpenFilesRequestPort>>,
    environment: Option<Arc<ProcessEnvironmentRequestPort>>,
    affinity: Option<Arc<ProcessAffinityRequestPort>>,
    affinity_control: Option<Arc<ProcessAffinityControlRequestPort>>,
    resource_control: Option<Arc<ProcessResourceControlRequestPort>>,
    network_escalation: Option<Arc<ProcessNetworkEscalationRequestPort>>,
}

impl ProcessFacets {
    #[must_use]
    pub fn with_list(mut self, port: Arc<ProcessListRequestPort>) -> Self {
        self.list = Some(port);
        self
    }

    #[must_use]
    pub fn with_control(mut self, port: Arc<ProcessControlRequestPort>) -> Self {
        self.control = Some(port);
        self
    }

    #[must_use]
    pub fn with_network(mut self, port: Arc<ProcessNetworkRequestPort>) -> Self {
        self.network = Some(port);
        self
    }

    #[must_use]
    pub fn with_gpu(mut self, port: Arc<ProcessGpuRequestPort>) -> Self {
        self.gpu = Some(port);
        self
    }

    #[must_use]
    pub fn with_resources(mut self, port: Arc<ProcessResourcesRequestPort>) -> Self {
        self.resources = Some(port);
        self
    }

    #[must_use]
    pub fn with_isolation(mut self, port: Arc<ProcessIsolationRequestPort>) -> Self {
        self.isolation = Some(port);
        self
    }

    #[must_use]
    pub fn with_threads(mut self, port: Arc<ProcessThreadsRequestPort>) -> Self {
        self.threads = Some(port);
        self
    }

    #[must_use]
    pub fn with_open_files(mut self, port: Arc<ProcessOpenFilesRequestPort>) -> Self {
        self.open_files = Some(port);
        self
    }

    #[must_use]
    pub fn with_environment(mut self, port: Arc<ProcessEnvironmentRequestPort>) -> Self {
        self.environment = Some(port);
        self
    }

    #[must_use]
    pub fn with_affinity(mut self, port: Arc<ProcessAffinityRequestPort>) -> Self {
        self.affinity = Some(port);
        self
    }

    #[must_use]
    pub fn with_affinity_control(mut self, port: Arc<ProcessAffinityControlRequestPort>) -> Self {
        self.affinity_control = Some(port);
        self
    }

    #[must_use]
    pub fn with_resource_control(mut self, port: Arc<ProcessResourceControlRequestPort>) -> Self {
        self.resource_control = Some(port);
        self
    }

    #[must_use]
    pub fn list(&self) -> Option<&ProcessListRequestPort> {
        self.list.as_deref()
    }

    #[must_use]
    pub fn control(&self) -> Option<&ProcessControlRequestPort> {
        self.control.as_deref()
    }

    #[must_use]
    pub fn network(&self) -> Option<&ProcessNetworkRequestPort> {
        self.network.as_deref()
    }

    #[must_use]
    pub fn gpu(&self) -> Option<&ProcessGpuRequestPort> {
        self.gpu.as_deref()
    }

    #[must_use]
    pub fn resources(&self) -> Option<&ProcessResourcesRequestPort> {
        self.resources.as_deref()
    }

    #[must_use]
    pub fn isolation(&self) -> Option<&ProcessIsolationRequestPort> {
        self.isolation.as_deref()
    }

    #[must_use]
    pub fn threads(&self) -> Option<&ProcessThreadsRequestPort> {
        self.threads.as_deref()
    }

    #[must_use]
    pub fn open_files(&self) -> Option<&ProcessOpenFilesRequestPort> {
        self.open_files.as_deref()
    }

    #[must_use]
    pub fn environment(&self) -> Option<&ProcessEnvironmentRequestPort> {
        self.environment.as_deref()
    }

    #[must_use]
    pub fn affinity(&self) -> Option<&ProcessAffinityRequestPort> {
        self.affinity.as_deref()
    }

    #[must_use]
    pub fn affinity_control(&self) -> Option<&ProcessAffinityControlRequestPort> {
        self.affinity_control.as_deref()
    }

    #[must_use]
    pub fn resource_control(&self) -> Option<&ProcessResourceControlRequestPort> {
        self.resource_control.as_deref()
    }

    #[must_use]
    pub fn with_network_escalation(
        mut self,
        port: Arc<ProcessNetworkEscalationRequestPort>,
    ) -> Self {
        self.network_escalation = Some(port);
        self
    }

    #[must_use]
    pub fn network_escalation(&self) -> Option<&ProcessNetworkEscalationRequestPort> {
        self.network_escalation.as_deref()
    }
}
