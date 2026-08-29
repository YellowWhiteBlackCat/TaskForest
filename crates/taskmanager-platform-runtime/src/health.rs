//! Capability health projection for typed observation snapshots.
//!
//! Aggregates independently fallible source and device-state outcomes into a
//! single `Available`/`Degraded`/`Unavailable` verdict without treating a
//! successful provider or a retained timestamp as proof of a trustworthy value.

use taskmanager_application::{ProcessInsightObservation, SystemTelemetryRevision};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{
    ContainerRollup, CpuTelemetryObservation, DeviceState, DeviceStatus, GpuTelemetryObservation,
    HostRuntimeObservation, MemoryTelemetryObservation, NetworkTelemetryObservation,
    ProcessEnvironment, ProcessGpuSnapshot, ProcessInsightSnapshot, ProcessIsolation,
    ProcessNetworkSnapshot, ProcessOpenFiles, ProcessResourceSnapshot, ProcessTelemetrySnapshot,
    ProcessThreads, StartupBootEvidenceSnapshot, StorageTelemetryObservation,
    SystemObservationState, SystemSnapshot,
};
use taskmanager_platform_contract::{
    CompositeSourceSnapshot, DeviceSourceSnapshot, PartialSourceSnapshot, ProviderFailure,
};

/// Health evidence attached to a successful domain event.
///
/// Provider execution success and observation quality are separate axes: an
/// adapter may successfully return a typed snapshot whose discovery or
/// enrichment sources are degraded or unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityHealth {
    Available,
    Degraded(FailureKind),
    Unavailable(ProviderFailure),
}

impl CapabilityHealth {
    #[must_use]
    pub const fn from_provider_result(result: Result<(), ProviderFailure>) -> Self {
        match result {
            Ok(()) => Self::Available,
            Err(failure) => Self::Unavailable(failure),
        }
    }
}

/// Health contract for one typed observation snapshot.
///
/// This is deliberately implemented by the runtime rather than by native
/// providers. A source-rich provider cannot make an `Ok(snapshot)` publication
/// look fully available without the snapshot's typed source/device evidence
/// being evaluated first.
pub trait ObservationHealth {
    fn observation_health(&self) -> CapabilityHealth;
}

/// Aggregate independently fallible source outcomes without confusing an
/// executed provider with a trustworthy observation.
#[must_use]
pub fn source_health(sources: &[SourceStatus]) -> CapabilityHealth {
    let mut has_successful_source = false;
    let mut strongest_failure = None;
    for source in sources {
        // `item_count` is diagnostic cardinality, not an availability signal.
        // A source may authoritatively observe zero items, or retain items
        // while its current refresh is unavailable.
        match source.outcome {
            SourceOutcome::Available | SourceOutcome::Empty => has_successful_source = true,
            SourceOutcome::Partial(failure) => {
                has_successful_source = true;
                retain_strongest(&mut strongest_failure, failure);
            }
            SourceOutcome::Unavailable(failure) => {
                retain_strongest(&mut strongest_failure, failure);
            }
        }
    }
    match (has_successful_source, strongest_failure) {
        (true, Some(failure)) => CapabilityHealth::Degraded(failure),
        (true, None) => CapabilityHealth::Available,
        (false, Some(failure)) => {
            CapabilityHealth::Unavailable(ProviderFailure::from_kind(failure))
        }
        (false, None) => CapabilityHealth::Unavailable(ProviderFailure::ProviderFault),
    }
}

/// Device discovery is authoritative independently from optional enrichments.
#[must_use]
pub fn device_source_health<T>(snapshot: &DeviceSourceSnapshot<T>) -> CapabilityHealth {
    match snapshot.discovery().outcome {
        SourceOutcome::Unavailable(failure) => {
            CapabilityHealth::Unavailable(ProviderFailure::from_kind(failure))
        }
        SourceOutcome::Partial(failure) => {
            if snapshot.enrichments.is_empty() {
                return CapabilityHealth::Degraded(failure);
            }
            let enrichment = source_health(&snapshot.enrichments);
            match enrichment {
                CapabilityHealth::Unavailable(enrichment_failure) => {
                    CapabilityHealth::Degraded(stronger_failure(failure, enrichment_failure.kind()))
                }
                CapabilityHealth::Degraded(enrichment_failure) => {
                    CapabilityHealth::Degraded(stronger_failure(failure, enrichment_failure))
                }
                CapabilityHealth::Available => CapabilityHealth::Degraded(failure),
            }
        }
        SourceOutcome::Available | SourceOutcome::Empty => {
            if snapshot.enrichments.is_empty() {
                CapabilityHealth::Available
            } else {
                source_health_with_authoritative_discovery(&snapshot.enrichments)
            }
        }
    }
}

/// Aggregate typed device/provider states without treating a believable value
/// or a retained timestamp as proof that the current observation succeeded.
#[must_use]
pub fn device_state_health(states: impl IntoIterator<Item = DeviceState>) -> CapabilityHealth {
    let mut has_successful_state = false;
    let mut strongest_failure = None;
    for state in states {
        match (state.status, state.last_success_ms) {
            (DeviceStatus::Healthy, Some(_)) => has_successful_state = true,
            (DeviceStatus::Healthy, None) => {
                retain_strongest(&mut strongest_failure, FailureKind::ProviderFault);
            }
            (status, _) => {
                if let Some(failure) = status.failure() {
                    retain_strongest(&mut strongest_failure, failure);
                }
            }
        }
    }
    match (has_successful_state, strongest_failure) {
        (true, Some(failure)) => CapabilityHealth::Degraded(failure),
        (true, None) => CapabilityHealth::Available,
        (false, Some(failure)) => {
            CapabilityHealth::Unavailable(ProviderFailure::from_kind(failure))
        }
        (false, None) => CapabilityHealth::Unavailable(ProviderFailure::ProviderFault),
    }
}

/// Health of an otherwise usable batch with zero or more partial failures.
///
/// An empty iterator means that no batch member failed and returns
/// [`CapabilityHealth::Available`]. A caller whose provider produced no usable
/// batch at all must report [`CapabilityHealth::Unavailable`] directly.
#[must_use]
pub fn degraded_health(failures: impl IntoIterator<Item = FailureKind>) -> CapabilityHealth {
    let mut strongest = None;
    for failure in failures {
        retain_strongest(&mut strongest, failure);
    }
    strongest.map_or(CapabilityHealth::Available, CapabilityHealth::Degraded)
}

impl<T> ObservationHealth for PartialSourceSnapshot<T> {
    fn observation_health(&self) -> CapabilityHealth {
        source_health(&self.sources)
    }
}

impl<T> ObservationHealth for CompositeSourceSnapshot<T> {
    fn observation_health(&self) -> CapabilityHealth {
        source_health(&self.sources)
    }
}

impl<T> ObservationHealth for DeviceSourceSnapshot<T> {
    fn observation_health(&self) -> CapabilityHealth {
        device_source_health(self)
    }
}

impl ObservationHealth for SystemSnapshot {
    fn observation_health(&self) -> CapabilityHealth {
        source_health(&self.telemetry_sources)
    }
}

impl ObservationHealth for StartupBootEvidenceSnapshot {
    fn observation_health(&self) -> CapabilityHealth {
        device_state_health([self.failed_units_state, self.critical_chain_state])
    }
}

impl ObservationHealth for ContainerRollup {
    fn observation_health(&self) -> CapabilityHealth {
        device_state_health([self.state])
    }
}

impl<T: ObservationHealth> ObservationHealth for (SystemTelemetryRevision, T) {
    fn observation_health(&self) -> CapabilityHealth {
        self.1.observation_health()
    }
}

macro_rules! impl_system_observation_health {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl ObservationHealth for $ty {
                fn observation_health(&self) -> CapabilityHealth {
                    system_observation_health(self.state(), self.sources())
                }
            }
        )+
    };
}

impl_system_observation_health!(
    HostRuntimeObservation,
    CpuTelemetryObservation,
    MemoryTelemetryObservation,
    StorageTelemetryObservation,
    NetworkTelemetryObservation,
    GpuTelemetryObservation,
);

fn system_observation_health(
    state: SystemObservationState,
    sources: &[SourceStatus],
) -> CapabilityHealth {
    match state {
        SystemObservationState::Current { .. } => source_health(sources),
        SystemObservationState::Partial { failure, .. } => match source_health(sources) {
            CapabilityHealth::Available => CapabilityHealth::Degraded(failure),
            CapabilityHealth::Degraded(source_failure) => {
                CapabilityHealth::Degraded(stronger_failure(failure, source_failure))
            }
            CapabilityHealth::Unavailable(source_failure) => {
                CapabilityHealth::Degraded(stronger_failure(failure, source_failure.kind()))
            }
        },
        SystemObservationState::Stale { failure, .. }
        | SystemObservationState::Unavailable { failure } => {
            CapabilityHealth::Unavailable(ProviderFailure::from_kind(failure))
        }
        SystemObservationState::Unknown => {
            CapabilityHealth::Unavailable(ProviderFailure::ProviderFault)
        }
    }
}

impl ObservationHealth for ProcessTelemetrySnapshot {
    fn observation_health(&self) -> CapabilityHealth {
        device_state_health([
            self.state,
            self.network.state,
            self.network.traffic_state,
            self.gpu.state,
            self.resources.state(),
            self.isolation.state,
            self.open_files.state,
            self.threads.state,
        ])
    }
}

impl ObservationHealth for ProcessInsightSnapshot<ProcessNetworkSnapshot> {
    fn observation_health(&self) -> CapabilityHealth {
        device_state_health([self.value.state, self.value.traffic_state])
    }
}

impl ObservationHealth for ProcessInsightSnapshot<ProcessGpuSnapshot> {
    fn observation_health(&self) -> CapabilityHealth {
        device_state_health([self.value.state])
    }
}

impl ObservationHealth for ProcessInsightSnapshot<ProcessResourceSnapshot> {
    fn observation_health(&self) -> CapabilityHealth {
        source_health(self.value.sources())
    }
}

impl ObservationHealth for ProcessInsightSnapshot<ProcessIsolation> {
    fn observation_health(&self) -> CapabilityHealth {
        device_state_health([self.value.state])
    }
}

impl ObservationHealth for ProcessInsightSnapshot<ProcessThreads> {
    fn observation_health(&self) -> CapabilityHealth {
        device_state_health([self.value.state])
    }
}

impl ObservationHealth for ProcessInsightSnapshot<ProcessOpenFiles> {
    fn observation_health(&self) -> CapabilityHealth {
        device_state_health([self.value.state])
    }
}

impl ObservationHealth for ProcessInsightSnapshot<ProcessEnvironment> {
    fn observation_health(&self) -> CapabilityHealth {
        device_state_health([self.value.state])
    }
}

impl ObservationHealth for ProcessInsightObservation<ProcessNetworkSnapshot> {
    fn observation_health(&self) -> CapabilityHealth {
        self.snapshot.observation_health()
    }
}

impl ObservationHealth for ProcessInsightObservation<ProcessGpuSnapshot> {
    fn observation_health(&self) -> CapabilityHealth {
        self.snapshot.observation_health()
    }
}

impl ObservationHealth for ProcessInsightObservation<ProcessResourceSnapshot> {
    fn observation_health(&self) -> CapabilityHealth {
        self.snapshot.observation_health()
    }
}

impl ObservationHealth for ProcessInsightObservation<ProcessIsolation> {
    fn observation_health(&self) -> CapabilityHealth {
        self.snapshot.observation_health()
    }
}

impl ObservationHealth for ProcessInsightObservation<ProcessThreads> {
    fn observation_health(&self) -> CapabilityHealth {
        self.snapshot.observation_health()
    }
}

impl ObservationHealth for ProcessInsightObservation<ProcessOpenFiles> {
    fn observation_health(&self) -> CapabilityHealth {
        self.snapshot.observation_health()
    }
}

impl ObservationHealth for ProcessInsightObservation<ProcessEnvironment> {
    fn observation_health(&self) -> CapabilityHealth {
        self.snapshot.observation_health()
    }
}

fn source_health_with_authoritative_discovery(sources: &[SourceStatus]) -> CapabilityHealth {
    match source_health(sources) {
        CapabilityHealth::Unavailable(failure) => CapabilityHealth::Degraded(failure.kind()),
        health => health,
    }
}

fn retain_strongest(current: &mut Option<FailureKind>, candidate: FailureKind) {
    if current.is_none_or(|failure| failure_priority(candidate) > failure_priority(failure)) {
        *current = Some(candidate);
    }
}

const fn stronger_failure(left: FailureKind, right: FailureKind) -> FailureKind {
    if failure_priority(left) >= failure_priority(right) {
        left
    } else {
        right
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        // Escalation-aware denial is the most actionable kind, so it wins a
        // merge over a generic denial or a transient sibling failure.
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::MissingDependency => 7,
        FailureKind::TimedOut => 6,
        FailureKind::ProviderFault => 5,
        FailureKind::TemporarilyUnavailable => 4,
        FailureKind::Unsupported => 3,
        FailureKind::IdentityChanged => 2,
        FailureKind::Rejected => 1,
    }
}

#[cfg(test)]
#[path = "../tests/headless/runtime_health_tests.rs"]
mod tests;
