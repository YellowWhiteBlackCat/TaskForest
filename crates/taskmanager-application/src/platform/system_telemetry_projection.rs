//! Application-owned merge policy for independently scheduled system domains.

use std::collections::{BTreeMap, HashSet};

use taskmanager_core::{
    CpuTelemetryObservation, DeviceId, DeviceLifecycle, FailureKind, GpuTelemetryObservation,
    HostRuntimeObservation, MemoryTelemetryObservation, NetworkTelemetryObservation, ProviderId,
    SourceOutcome, SourceStatus, StorageTelemetryObservation, SystemObservationState,
    SystemSnapshot, SystemTelemetryDomains,
};
use taskmanager_platform_contract::SubmissionErrorKind;

use super::event_batch::CorrelatedSystemTelemetryOutcome;
use super::facets::SystemTelemetryDomainOutcome;
use super::{SystemTelemetryDomainEvent, SystemTelemetryRevision};
use crate::device_lifecycle::{
    DeviceLifecycleDiagnosticHistory, DeviceLifecycleProjection, DeviceLifecycleSnapshotRevision,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemTelemetryDomain {
    Host,
    Cpu,
    Memory,
    Storage,
    Network,
    Gpu,
}

impl SystemTelemetryDomain {
    pub const ALL: [Self; 6] = [
        Self::Host,
        Self::Cpu,
        Self::Memory,
        Self::Storage,
        Self::Network,
        Self::Gpu,
    ];

    #[must_use]
    pub const fn capability(self) -> taskmanager_platform_contract::CapabilityId {
        match self {
            Self::Host => taskmanager_platform_contract::CapabilityId::TELEMETRY_HOST,
            Self::Cpu => taskmanager_platform_contract::CapabilityId::TELEMETRY_CPU,
            Self::Memory => taskmanager_platform_contract::CapabilityId::TELEMETRY_MEMORY,
            Self::Storage => taskmanager_platform_contract::CapabilityId::TELEMETRY_STORAGE,
            Self::Network => taskmanager_platform_contract::CapabilityId::TELEMETRY_NETWORK,
            Self::Gpu => taskmanager_platform_contract::CapabilityId::TELEMETRY_GPU,
        }
    }

    #[must_use]
    pub fn from_capability(
        capability: &taskmanager_platform_contract::CapabilityId,
    ) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|domain| domain.capability() == *capability)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemTelemetryUnavailable {
    Provider(FailureKind),
    Submission(SubmissionErrorKind),
}

#[derive(Clone, Debug, Default)]
pub enum SystemTelemetryDomainState<T> {
    #[default]
    Pending,
    Current(T),
    Partial(T),
    Stale(T),
    Unavailable {
        observation: Option<T>,
        reason: SystemTelemetryUnavailable,
    },
}

impl<T> SystemTelemetryDomainState<T> {
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }

    #[must_use]
    pub const fn is_current(&self) -> bool {
        matches!(self, Self::Current(_) | Self::Partial(_))
    }

    /// The value a renderer may show for this domain, through the shared
    /// staleness fold: a current or partial domain reads the live observation,
    /// while a stale — or unavailable-but-once-observed — domain keeps its last
    /// known one. A domain that stopped refreshing therefore degrades to its
    /// previous fact instead of regressing into a fabricated zero.
    ///
    /// `Pending`, and an unavailable domain that never observed anything, have
    /// nothing honest to show: `None` tells the caller to render its missing
    /// marker, never a placeholder value.
    ///
    /// The accessors are function pointers so the fold stays observation-type
    /// neutral; every domain's own `current_value`/`last_known_value` pair is
    /// the intended argument.
    #[must_use]
    pub fn usable<'a, V>(
        &'a self,
        current: fn(&'a T) -> Option<V>,
        last_known: fn(&'a T) -> Option<V>,
    ) -> Option<V> {
        match self {
            Self::Current(observation) | Self::Partial(observation) => current(observation),
            Self::Stale(observation)
            | Self::Unavailable {
                observation: Some(observation),
                ..
            } => last_known(observation),
            Self::Pending
            | Self::Unavailable {
                observation: None, ..
            } => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProjectedSystemTelemetry {
    pub revision: SystemTelemetryRevision,
    pub host: SystemTelemetryDomainState<HostRuntimeObservation>,
    pub cpu: SystemTelemetryDomainState<CpuTelemetryObservation>,
    pub memory: SystemTelemetryDomainState<MemoryTelemetryObservation>,
    pub storage: SystemTelemetryDomainState<StorageTelemetryObservation>,
    pub network: SystemTelemetryDomainState<NetworkTelemetryObservation>,
    pub gpu: SystemTelemetryDomainState<GpuTelemetryObservation>,
}

impl ProjectedSystemTelemetry {
    pub(crate) fn pending(revision: SystemTelemetryRevision) -> Self {
        Self {
            revision,
            host: SystemTelemetryDomainState::Pending,
            cpu: SystemTelemetryDomainState::Pending,
            memory: SystemTelemetryDomainState::Pending,
            storage: SystemTelemetryDomainState::Pending,
            network: SystemTelemetryDomainState::Pending,
            gpu: SystemTelemetryDomainState::Pending,
        }
    }

    #[must_use]
    pub fn is_terminal(&self) -> bool {
        !self.host.is_pending()
            && !self.cpu.is_pending()
            && !self.memory.is_pending()
            && !self.storage.is_pending()
            && !self.network.is_pending()
            && !self.gpu.is_pending()
    }

    #[must_use]
    pub fn has_current_domain(&self) -> bool {
        self.host.is_current()
            || self.cpu.is_current()
            || self.memory.is_current()
            || self.storage.is_current()
            || self.network.is_current()
            || self.gpu.is_current()
    }

    /// Assemble the complete render read model only when every domain supplied
    /// current facts and all host scalars are current.
    ///
    /// `SystemSnapshot` cannot represent per-domain absence without believable
    /// default values, so stale/unavailable domains cannot form this snapshot.
    #[must_use]
    pub fn complete_snapshot(&self) -> Option<SystemSnapshot> {
        let domains = SystemTelemetryDomains {
            host: current_observation(&self.host)?.clone(),
            cpu: current_observation(&self.cpu)?.clone(),
            memory: current_observation(&self.memory)?.clone(),
            storage: current_observation(&self.storage)?.clone(),
            network: current_observation(&self.network)?.clone(),
            gpu: current_observation(&self.gpu)?.clone(),
        };
        SystemSnapshot::from_current_domains(&domains)
    }

    /// Build the frontend render model once the required domains are current.
    /// Optional host-thread and GPU facets remain typed gaps instead of
    /// blanking otherwise usable CPU, memory, storage, and network data.
    #[must_use]
    pub fn render_snapshot(&self) -> Option<SystemSnapshot> {
        let domains = SystemTelemetryDomains {
            host: current_observation(&self.host)?.clone(),
            cpu: current_observation(&self.cpu)?.clone(),
            memory: current_observation(&self.memory)?.clone(),
            storage: current_observation(&self.storage)?.clone(),
            network: current_observation(&self.network)?.clone(),
            gpu: match &self.gpu {
                SystemTelemetryDomainState::Current(observation)
                | SystemTelemetryDomainState::Partial(observation) => observation.clone(),
                SystemTelemetryDomainState::Unavailable {
                    observation: Some(observation),
                    ..
                } => observation.clone(),
                SystemTelemetryDomainState::Unavailable {
                    observation: None,
                    reason,
                } => {
                    let failure = match reason {
                        SystemTelemetryUnavailable::Provider(failure) => *failure,
                        SystemTelemetryUnavailable::Submission(_) => {
                            FailureKind::TemporarilyUnavailable
                        }
                    };
                    GpuTelemetryObservation::unavailable(
                        failure,
                        vec![SourceStatus {
                            provider: ProviderId::borrowed("application.system.gpu"),
                            outcome: SourceOutcome::Unavailable(failure),
                            item_count: 0,
                        }],
                        Vec::new(),
                        BTreeMap::new(),
                    )
                }
                SystemTelemetryDomainState::Pending | SystemTelemetryDomainState::Stale(_) => {
                    return None;
                }
            },
        };
        SystemSnapshot::from_available_domains(&domains)
    }

    /// Monotonicity policy shared by every frontend (GUI + TUI): `incoming`
    /// strictly extends `current` when its revision is higher, or when the
    /// same revision never lets a resolved domain regress to Pending.
    ///
    /// Both frontends must apply the SAME rule so acceptance cannot drift;
    /// this is the single implementation (no copy-paste policy).
    #[must_use]
    pub fn extends(&self, incoming: &Self) -> bool {
        if incoming.revision != self.revision {
            return incoming.revision > self.revision;
        }
        [
            (!self.host.is_pending(), incoming.host.is_pending()),
            (!self.cpu.is_pending(), incoming.cpu.is_pending()),
            (!self.memory.is_pending(), incoming.memory.is_pending()),
            (!self.storage.is_pending(), incoming.storage.is_pending()),
            (!self.network.is_pending(), incoming.network.is_pending()),
            (!self.gpu.is_pending(), incoming.gpu.is_pending()),
        ]
        .into_iter()
        .all(|(was_resolved, is_pending)| !was_resolved || !is_pending)
    }
}

/// Result of applying one monotonic projection against the latest state.
#[derive(Clone, Debug)]
pub enum ProjectionAcceptance {
    /// The incoming projection does not strictly extend the current one
    /// (older revision, or a resolved domain regressing to pending). Nothing
    /// changed; `latest` was left untouched.
    Rejected,
    /// The incoming projection is the new latest typed state. `snapshot` is
    /// the render snapshot when the required domains supplied current facts
    /// (the frontend decides what `None` means for its own render model).
    Accepted {
        snapshot: Option<Box<SystemSnapshot>>,
    },
}

impl ProjectedSystemTelemetry {
    /// Apply the shared acceptance policy: store `incoming` as the latest
    /// typed projection when it [`extends`](Self::extends) the current one,
    /// and surface the independently renderable snapshot when it can be
    /// assembled.
    ///
    /// This is the ONLY write path for `latest` — frontends map the typed
    /// result onto their own cache (GUI keeps a concrete `SystemSnapshot`,
    /// TUI keeps an `Option<SystemSnapshot>`); neither frontend reimplements
    /// the monotonicity rule.
    pub fn accept_projection(latest: &mut Option<Self>, incoming: Self) -> ProjectionAcceptance {
        if latest
            .as_ref()
            .is_some_and(|current| !current.extends(&incoming))
        {
            return ProjectionAcceptance::Rejected;
        }
        let snapshot = incoming.render_snapshot();
        *latest = Some(incoming);
        ProjectionAcceptance::Accepted {
            snapshot: snapshot.map(Box::new),
        }
    }
}

/// Apply device lifecycle sidecars from accepted storage/network/GPU observed
/// outcomes. CPU, memory, and host events intentionally have no lifecycle
/// effect, and no aggregate `SystemSnapshot` is synthesized.
///
/// Shared by every frontend: an unavailable runtime completion advances
/// projection/history elsewhere but carries no sidecar and therefore cannot
/// invent presence or absence.
pub fn apply_system_outcome_lifecycle(
    projection: &mut DeviceLifecycleProjection,
    diagnostics: &mut DeviceLifecycleDiagnosticHistory,
    correlated: &CorrelatedSystemTelemetryOutcome,
) {
    let SystemTelemetryDomainOutcome::Observed(event) = &correlated.event else {
        return;
    };
    let revision = DeviceLifecycleSnapshotRevision::new(correlated.sequence.get());
    let result = match event {
        SystemTelemetryDomainEvent::Storage { observation, .. } => {
            Some(projection.apply_storage_telemetry_observation(revision, observation))
        }
        SystemTelemetryDomainEvent::Network { observation, .. } => {
            Some(projection.apply_network_telemetry_observation(revision, observation))
        }
        SystemTelemetryDomainEvent::Gpu { observation, .. } => {
            Some(projection.apply_gpu_telemetry_observation(revision, observation))
        }
        SystemTelemetryDomainEvent::Host { .. }
        | SystemTelemetryDomainEvent::Cpu { .. }
        | SystemTelemetryDomainEvent::Memory { .. } => None,
    };
    if let Some(result) = result {
        diagnostics.record(result);
    }
}

impl<T> SystemTelemetryDomainState<T> {
    fn observation(&self) -> Option<&T> {
        match self {
            Self::Pending
            | Self::Unavailable {
                observation: None, ..
            } => None,
            Self::Current(observation)
            | Self::Partial(observation)
            | Self::Stale(observation)
            | Self::Unavailable {
                observation: Some(observation),
                ..
            } => Some(observation),
        }
    }
}

fn current_observation<T>(state: &SystemTelemetryDomainState<T>) -> Option<&T> {
    match state {
        SystemTelemetryDomainState::Current(observation)
        | SystemTelemetryDomainState::Partial(observation) => Some(observation),
        SystemTelemetryDomainState::Pending
        | SystemTelemetryDomainState::Stale(_)
        | SystemTelemetryDomainState::Unavailable { .. } => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemTelemetryProjectionRejection {
    NoActiveRequest,
    StaleOrUnexpectedRevision,
    DuplicateDomain,
    ConflictingDeviceLifecycle,
}

#[derive(Clone, Debug)]
pub enum SystemTelemetryProjectionApplyResult {
    AppliedPartial(Box<ProjectedSystemTelemetry>),
    AppliedTerminal {
        projection: Box<ProjectedSystemTelemetry>,
    },
    Ignored(SystemTelemetryProjectionRejection),
}

#[derive(Clone, Debug, Default)]
pub struct SystemTelemetryProjection {
    current: Option<ProjectedSystemTelemetry>,
}

impl SystemTelemetryProjection {
    pub fn begin(&mut self, revision: SystemTelemetryRevision) {
        self.begin_domains(revision, &SystemTelemetryDomain::ALL);
    }

    pub fn begin_domains(
        &mut self,
        revision: SystemTelemetryRevision,
        domains: &[SystemTelemetryDomain],
    ) {
        if self
            .current
            .as_ref()
            .is_none_or(|current| current.revision < revision)
        {
            let mut next = self
                .current
                .clone()
                .unwrap_or_else(|| ProjectedSystemTelemetry::pending(revision));
            next.revision = revision;
            for domain in domains {
                set_pending(&mut next, *domain);
            }
            self.current = Some(next);
        }
    }

    #[must_use]
    pub fn current(&self) -> Option<&ProjectedSystemTelemetry> {
        self.current.as_ref()
    }

    #[must_use]
    pub fn snapshot(&self) -> Option<ProjectedSystemTelemetry> {
        self.current.clone()
    }

    pub fn apply(
        &mut self,
        event: &SystemTelemetryDomainEvent,
    ) -> SystemTelemetryProjectionApplyResult {
        let revision = event.revision();
        let domain = event.domain();
        if let Err(rejection) = self.prepare(revision, domain, event) {
            return SystemTelemetryProjectionApplyResult::Ignored(rejection);
        }
        let Some(current) = self.current.as_mut() else {
            return SystemTelemetryProjectionApplyResult::Ignored(
                SystemTelemetryProjectionRejection::NoActiveRequest,
            );
        };
        match event {
            SystemTelemetryDomainEvent::Host { observation, .. } => {
                current.host = classify((**observation).clone());
            }
            SystemTelemetryDomainEvent::Cpu { observation, .. } => {
                current.cpu = classify((**observation).clone());
            }
            SystemTelemetryDomainEvent::Memory { observation, .. } => {
                current.memory = classify((**observation).clone());
            }
            SystemTelemetryDomainEvent::Storage { observation, .. } => {
                current.storage = classify((**observation).clone());
            }
            SystemTelemetryDomainEvent::Network { observation, .. } => {
                current.network = classify((**observation).clone());
            }
            SystemTelemetryDomainEvent::Gpu { observation, .. } => {
                current.gpu = classify((**observation).clone());
            }
        }
        Self::applied(current)
    }

    pub fn apply_failure(
        &mut self,
        revision: SystemTelemetryRevision,
        domain: SystemTelemetryDomain,
        reason: SystemTelemetryUnavailable,
    ) -> SystemTelemetryProjectionApplyResult {
        let Some(current) = self.current.as_mut() else {
            return SystemTelemetryProjectionApplyResult::Ignored(
                SystemTelemetryProjectionRejection::NoActiveRequest,
            );
        };
        if current.revision != revision {
            return SystemTelemetryProjectionApplyResult::Ignored(
                SystemTelemetryProjectionRejection::StaleOrUnexpectedRevision,
            );
        }
        if !domain_is_pending(current, domain) {
            return SystemTelemetryProjectionApplyResult::Ignored(
                SystemTelemetryProjectionRejection::DuplicateDomain,
            );
        }
        set_unavailable(current, domain, reason);
        Self::applied(current)
    }

    fn prepare(
        &self,
        revision: SystemTelemetryRevision,
        domain: SystemTelemetryDomain,
        event: &SystemTelemetryDomainEvent,
    ) -> Result<(), SystemTelemetryProjectionRejection> {
        let Some(current) = self.current.as_ref() else {
            return Err(SystemTelemetryProjectionRejection::NoActiveRequest);
        };
        if current.revision != revision {
            return Err(SystemTelemetryProjectionRejection::StaleOrUnexpectedRevision);
        }
        if !domain_is_pending(current, domain) {
            return Err(SystemTelemetryProjectionRejection::DuplicateDomain);
        }
        if lifecycle_conflicts(current, event) {
            return Err(SystemTelemetryProjectionRejection::ConflictingDeviceLifecycle);
        }
        Ok(())
    }

    fn applied(current: &ProjectedSystemTelemetry) -> SystemTelemetryProjectionApplyResult {
        if current.is_terminal() {
            SystemTelemetryProjectionApplyResult::AppliedTerminal {
                projection: Box::new(current.clone()),
            }
        } else {
            SystemTelemetryProjectionApplyResult::AppliedPartial(Box::new(current.clone()))
        }
    }
}

fn set_pending(projection: &mut ProjectedSystemTelemetry, domain: SystemTelemetryDomain) {
    match domain {
        SystemTelemetryDomain::Host => projection.host = SystemTelemetryDomainState::Pending,
        SystemTelemetryDomain::Cpu => projection.cpu = SystemTelemetryDomainState::Pending,
        SystemTelemetryDomain::Memory => projection.memory = SystemTelemetryDomainState::Pending,
        SystemTelemetryDomain::Storage => {
            projection.storage = SystemTelemetryDomainState::Pending;
        }
        SystemTelemetryDomain::Network => projection.network = SystemTelemetryDomainState::Pending,
        SystemTelemetryDomain::Gpu => projection.gpu = SystemTelemetryDomainState::Pending,
    }
}

trait ClassifiableObservation {
    fn state(&self) -> SystemObservationState;
}

macro_rules! impl_classifiable {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl ClassifiableObservation for $ty {
                fn state(&self) -> SystemObservationState {
                    self.state()
                }
            }
        )+
    };
}

impl_classifiable!(
    HostRuntimeObservation,
    CpuTelemetryObservation,
    MemoryTelemetryObservation,
    StorageTelemetryObservation,
    NetworkTelemetryObservation,
    GpuTelemetryObservation,
);

fn classify<T: ClassifiableObservation>(observation: T) -> SystemTelemetryDomainState<T> {
    match observation.state() {
        SystemObservationState::Current { .. } => SystemTelemetryDomainState::Current(observation),
        SystemObservationState::Partial { .. } => SystemTelemetryDomainState::Partial(observation),
        SystemObservationState::Stale { .. } => SystemTelemetryDomainState::Stale(observation),
        SystemObservationState::Unavailable { failure } => {
            SystemTelemetryDomainState::Unavailable {
                observation: Some(observation),
                reason: SystemTelemetryUnavailable::Provider(failure),
            }
        }
        SystemObservationState::Unknown => SystemTelemetryDomainState::Unavailable {
            observation: Some(observation),
            reason: SystemTelemetryUnavailable::Provider(FailureKind::ProviderFault),
        },
    }
}

fn domain_is_pending(current: &ProjectedSystemTelemetry, domain: SystemTelemetryDomain) -> bool {
    match domain {
        SystemTelemetryDomain::Host => current.host.is_pending(),
        SystemTelemetryDomain::Cpu => current.cpu.is_pending(),
        SystemTelemetryDomain::Memory => current.memory.is_pending(),
        SystemTelemetryDomain::Storage => current.storage.is_pending(),
        SystemTelemetryDomain::Network => current.network.is_pending(),
        SystemTelemetryDomain::Gpu => current.gpu.is_pending(),
    }
}

fn set_unavailable(
    current: &mut ProjectedSystemTelemetry,
    domain: SystemTelemetryDomain,
    reason: SystemTelemetryUnavailable,
) {
    match domain {
        SystemTelemetryDomain::Host => {
            current.host = SystemTelemetryDomainState::Unavailable {
                observation: None,
                reason,
            };
        }
        SystemTelemetryDomain::Cpu => {
            current.cpu = SystemTelemetryDomainState::Unavailable {
                observation: None,
                reason,
            };
        }
        SystemTelemetryDomain::Memory => {
            current.memory = SystemTelemetryDomainState::Unavailable {
                observation: None,
                reason,
            };
        }
        SystemTelemetryDomain::Storage => {
            current.storage = SystemTelemetryDomainState::Unavailable {
                observation: None,
                reason,
            };
        }
        SystemTelemetryDomain::Network => {
            current.network = SystemTelemetryDomainState::Unavailable {
                observation: None,
                reason,
            };
        }
        SystemTelemetryDomain::Gpu => {
            current.gpu = SystemTelemetryDomainState::Unavailable {
                observation: None,
                reason,
            };
        }
    }
}

fn lifecycle_conflicts(
    current: &ProjectedSystemTelemetry,
    incoming: &SystemTelemetryDomainEvent,
) -> bool {
    let mut known = HashSet::<&str>::new();
    for lifecycles in [
        lifecycle_map(&current.storage),
        lifecycle_map(&current.network),
        lifecycle_map(&current.gpu),
    ]
    .into_iter()
    .flatten()
    {
        for (id, lifecycle) in lifecycles {
            let _ = lifecycle;
            known.insert(id.as_str());
        }
    }
    incoming_lifecycles(incoming)
        .is_some_and(|lifecycles| lifecycles.keys().any(|id| known.contains(id.as_str())))
}

fn lifecycle_map<T: DeviceDomain>(
    state: &SystemTelemetryDomainState<T>,
) -> Option<&std::collections::BTreeMap<DeviceId, DeviceLifecycle>> {
    state.observation().map(DeviceDomain::device_lifecycles)
}

trait DeviceDomain {
    fn device_lifecycles(&self) -> &std::collections::BTreeMap<DeviceId, DeviceLifecycle>;
}

macro_rules! impl_device_domain {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl DeviceDomain for $ty {
                fn device_lifecycles(
                    &self,
                ) -> &std::collections::BTreeMap<DeviceId, DeviceLifecycle> {
                    self.device_lifecycles()
                }
            }
        )+
    };
}

impl_device_domain!(
    StorageTelemetryObservation,
    NetworkTelemetryObservation,
    GpuTelemetryObservation,
);

fn incoming_lifecycles(
    event: &SystemTelemetryDomainEvent,
) -> Option<&std::collections::BTreeMap<DeviceId, DeviceLifecycle>> {
    match event {
        SystemTelemetryDomainEvent::Storage { observation, .. } => {
            Some(observation.device_lifecycles())
        }
        SystemTelemetryDomainEvent::Network { observation, .. } => {
            Some(observation.device_lifecycles())
        }
        SystemTelemetryDomainEvent::Gpu { observation, .. } => {
            Some(observation.device_lifecycles())
        }
        SystemTelemetryDomainEvent::Host { .. }
        | SystemTelemetryDomainEvent::Cpu { .. }
        | SystemTelemetryDomainEvent::Memory { .. } => None,
    }
}

#[cfg(test)]
#[path = "../../tests/headless/platform/system_telemetry_projection.rs"]
mod tests;
