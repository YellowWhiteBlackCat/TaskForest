//! Process resource telemetry: independently fallible resource facts (rlimit
//! limits, resource-group membership, memory/CPU/process accounting) as typed
//! observations with availability, plus the resource-group limit and membership
//! model and the per-process resource snapshot.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::core::device_state::DeviceState;
use crate::core::identity::ProviderId;
use crate::core::{FailureKind, SourceStatus};

/// The last successful resource fact retained by a stale observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum ResourceLastObservation<T> {
    Value(T),
    Absent,
}

/// One independently fallible resource fact.
///
/// This sum type cannot express contradictory states such as a current
/// observation without a value. `LimitValue::Unlimited` is a normal current
/// value with its own observation and last-success time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ResourceObservation<T> {
    /// Compatibility state for snapshots written before typed resource facts.
    #[default]
    Unknown,
    Current {
        value: T,
        observed_at_ms: u64,
    },
    Partial {
        value: T,
        observed_at_ms: u64,
        failure: FailureKind,
    },
    /// A successful observation proved that the fact does not exist.
    Absent {
        observed_at_ms: u64,
    },
    Stale {
        last: ResourceLastObservation<T>,
        last_success_ms: u64,
        failure: FailureKind,
    },
    Unavailable {
        failure: FailureKind,
    },
}

impl<T> ResourceObservation<T> {
    #[must_use]
    pub const fn current(value: T, observed_at_ms: u64) -> Self {
        Self::Current {
            value,
            observed_at_ms,
        }
    }

    #[must_use]
    pub const fn absent(observed_at_ms: u64) -> Self {
        Self::Absent { observed_at_ms }
    }

    #[must_use]
    pub const fn partial(value: T, observed_at_ms: u64, failure: FailureKind) -> Self {
        Self::Partial {
            value,
            observed_at_ms,
            failure,
        }
    }

    #[must_use]
    pub const fn unavailable(failure: FailureKind) -> Self {
        Self::Unavailable { failure }
    }

    #[must_use]
    pub fn transition_failure(self, failure: FailureKind) -> Self {
        match self {
            Self::Current {
                value,
                observed_at_ms,
            }
            | Self::Partial {
                value,
                observed_at_ms,
                ..
            } => Self::Stale {
                last: ResourceLastObservation::Value(value),
                last_success_ms: observed_at_ms,
                failure,
            },
            Self::Absent { observed_at_ms } => Self::Stale {
                last: ResourceLastObservation::Absent,
                last_success_ms: observed_at_ms,
                failure,
            },
            Self::Stale {
                last,
                last_success_ms,
                ..
            } => Self::Stale {
                last,
                last_success_ms,
                failure,
            },
            Self::Unknown | Self::Unavailable { .. } => Self::Unavailable { failure },
        }
    }

    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        match self {
            Self::Unavailable { failure } => previous.transition_failure(failure),
            _ => self,
        }
    }

    #[must_use]
    pub const fn current_value(&self) -> Option<&T> {
        match self {
            Self::Current { value, .. } | Self::Partial { value, .. } => Some(value),
            Self::Unknown | Self::Absent { .. } | Self::Stale { .. } | Self::Unavailable { .. } => {
                None
            }
        }
    }

    #[must_use]
    pub const fn last_known_value(&self) -> Option<&T> {
        match self {
            Self::Current { value, .. }
            | Self::Partial { value, .. }
            | Self::Stale {
                last: ResourceLastObservation::Value(value),
                ..
            } => Some(value),
            Self::Unknown
            | Self::Absent { .. }
            | Self::Stale {
                last: ResourceLastObservation::Absent,
                ..
            }
            | Self::Unavailable { .. } => None,
        }
    }

    #[must_use]
    pub const fn last_success_ms(&self) -> Option<u64> {
        match self {
            Self::Current { observed_at_ms, .. }
            | Self::Partial { observed_at_ms, .. }
            | Self::Absent { observed_at_ms } => Some(*observed_at_ms),
            Self::Stale {
                last_success_ms, ..
            } => Some(*last_success_ms),
            Self::Unknown | Self::Unavailable { .. } => None,
        }
    }

    #[must_use]
    pub const fn failure(&self) -> Option<FailureKind> {
        match self {
            Self::Partial { failure, .. }
            | Self::Stale { failure, .. }
            | Self::Unavailable { failure } => Some(*failure),
            Self::Unknown | Self::Current { .. } | Self::Absent { .. } => None,
        }
    }

    #[must_use]
    pub const fn is_current(&self) -> bool {
        matches!(
            self,
            Self::Current { .. } | Self::Partial { .. } | Self::Absent { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceLimitKind {
    CpuTime,
    FileSize,
    DataSize,
    StackSize,
    CoreFileSize,
    ResidentSet,
    Processes,
    OpenFiles,
    LockedMemory,
    AddressSpace,
    FileLocks,
    PendingSignals,
    MessageQueue,
    NicePriority,
    RealtimePriority,
    RealtimeTimeout,
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitValue {
    Unlimited,
    Value(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimit {
    pub kind: ResourceLimitKind,
    pub soft: LimitValue,
    pub hard: LimitValue,
    pub unit: Option<String>,
}

/// Platform-neutral resource-group CPU quota update.
///
/// Linux maps this to cgroup-v2 `cpu.max`; another adapter may translate the
/// same intent to its native job-object or process-group primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceGroupCpuLimit {
    pub quota: LimitValue,
    pub period_micros: u64,
}

/// Platform-neutral resource-group limit update.
///
/// The field names describe the shared intent. Native providers own the
/// concrete files, hierarchy names, authorization and rollback semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResourceGroupLimitRequest {
    pub memory: Option<LimitValue>,
    pub cpu: Option<ResourceGroupCpuLimit>,
    pub processes: Option<LimitValue>,
}

/// Provider-neutral resource-group membership.
///
/// Linux maps cgroup membership here. Other platforms may expose job objects,
/// coalitions, containers, or an equivalent grouping primitive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceGroupMembership {
    pub provider: ProviderId,
    /// Provider-native hierarchy or namespace identifier, when one exists.
    #[serde(default, rename = "hierarchy_id", alias = "native_hierarchy_id")]
    pub native_hierarchy_id: Option<u32>,
    /// Resource-control capabilities attached to this membership.
    #[serde(default, rename = "controllers", alias = "capabilities")]
    pub capabilities: Vec<String>,
    /// Opaque provider-native locator. Native control code may resolve it only
    /// after revalidating the process identity.
    #[serde(rename = "path", alias = "native_locator")]
    pub native_locator: String,
}

/// Typed truth behind legacy process-resource projections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProcessResourceObservations {
    pub limits: ResourceObservation<Vec<ResourceLimit>>,
    pub resource_groups: ResourceObservation<Vec<ResourceGroupMembership>>,
    pub memory_usage_bytes: ResourceObservation<u64>,
    pub memory_limit: ResourceObservation<LimitValue>,
    pub cpu_time_quota_micros: ResourceObservation<LimitValue>,
    pub cpu_time_period_micros: ResourceObservation<u64>,
    pub process_count: ResourceObservation<u64>,
    pub process_limit: ResourceObservation<LimitValue>,
}

impl ProcessResourceObservations {
    #[must_use]
    fn retain_previous(self, previous: Self, same_resource_groups: bool) -> Self {
        Self {
            limits: self.limits.retain_previous(previous.limits),
            resource_groups: self
                .resource_groups
                .retain_previous(previous.resource_groups),
            memory_usage_bytes: retain_group_observation(
                self.memory_usage_bytes,
                previous.memory_usage_bytes,
                same_resource_groups,
            ),
            memory_limit: retain_group_observation(
                self.memory_limit,
                previous.memory_limit,
                same_resource_groups,
            ),
            cpu_time_quota_micros: retain_group_observation(
                self.cpu_time_quota_micros,
                previous.cpu_time_quota_micros,
                same_resource_groups,
            ),
            cpu_time_period_micros: retain_group_observation(
                self.cpu_time_period_micros,
                previous.cpu_time_period_micros,
                same_resource_groups,
            ),
            process_count: retain_group_observation(
                self.process_count,
                previous.process_count,
                same_resource_groups,
            ),
            process_limit: retain_group_observation(
                self.process_limit,
                previous.process_limit,
                same_resource_groups,
            ),
        }
    }
}

fn retain_group_observation<T>(
    current: ResourceObservation<T>,
    previous: ResourceObservation<T>,
    same_resource_groups: bool,
) -> ResourceObservation<T> {
    if same_resource_groups {
        current.retain_previous(previous)
    } else {
        current
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProcessResourceSnapshot {
    state: DeviceState,
    observations: ProcessResourceObservations,
    sources: Vec<SourceStatus>,
}

/// Schema-v1 compatibility shape. Legacy mirrors exist only at this serde
/// boundary; the domain snapshot stores typed observations exclusively.
#[derive(Serialize, Deserialize, Default)]
struct ProcessResourceSnapshotWire {
    #[serde(default)]
    state: DeviceState,
    #[serde(default)]
    limits: Vec<ResourceLimit>,
    #[serde(default, rename = "groups", alias = "resource_groups")]
    resource_groups: Vec<ResourceGroupMembership>,
    #[serde(default, rename = "memory_current_bytes", alias = "memory_usage_bytes")]
    memory_usage_bytes: Option<u64>,
    #[serde(default, rename = "memory_max", alias = "memory_limit")]
    memory_limit: Option<LimitValue>,
    #[serde(default, rename = "cpu_quota_us", alias = "cpu_time_quota_micros")]
    cpu_time_quota_micros: Option<LimitValue>,
    #[serde(default, rename = "cpu_period_us", alias = "cpu_time_period_micros")]
    cpu_time_period_micros: Option<u64>,
    #[serde(default, rename = "pids_current", alias = "process_count")]
    process_count: Option<u64>,
    #[serde(default, rename = "pids_max", alias = "process_limit")]
    process_limit: Option<LimitValue>,
    #[serde(default)]
    observations: ProcessResourceObservations,
    #[serde(default)]
    sources: Vec<SourceStatus>,
}

impl Serialize for ProcessResourceSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ProcessResourceSnapshotWire {
            state: self.state,
            limits: wire_current_list(&self.observations.limits),
            resource_groups: wire_current_list(&self.observations.resource_groups),
            memory_usage_bytes: wire_current_copy(&self.observations.memory_usage_bytes),
            memory_limit: wire_current_copy(&self.observations.memory_limit),
            cpu_time_quota_micros: wire_current_copy(&self.observations.cpu_time_quota_micros),
            cpu_time_period_micros: wire_current_copy(&self.observations.cpu_time_period_micros),
            process_count: wire_current_copy(&self.observations.process_count),
            process_limit: wire_current_copy(&self.observations.process_limit),
            observations: self.observations.clone(),
            sources: self.sources.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProcessResourceSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ProcessResourceSnapshotWire::deserialize(deserializer)?;
        let observed_at_ms = wire.state.last_success_ms;
        let observations = ProcessResourceObservations {
            limits: hydrate_legacy_list(wire.observations.limits, wire.limits, observed_at_ms),
            resource_groups: hydrate_legacy_list(
                wire.observations.resource_groups,
                wire.resource_groups,
                observed_at_ms,
            ),
            memory_usage_bytes: hydrate_legacy_value(
                wire.observations.memory_usage_bytes,
                wire.memory_usage_bytes,
                observed_at_ms,
            ),
            memory_limit: hydrate_legacy_value(
                wire.observations.memory_limit,
                wire.memory_limit,
                observed_at_ms,
            ),
            cpu_time_quota_micros: hydrate_legacy_value(
                wire.observations.cpu_time_quota_micros,
                wire.cpu_time_quota_micros,
                observed_at_ms,
            ),
            cpu_time_period_micros: hydrate_legacy_value(
                wire.observations.cpu_time_period_micros,
                wire.cpu_time_period_micros,
                observed_at_ms,
            ),
            process_count: hydrate_legacy_value(
                wire.observations.process_count,
                wire.process_count,
                observed_at_ms,
            ),
            process_limit: hydrate_legacy_value(
                wire.observations.process_limit,
                wire.process_limit,
                observed_at_ms,
            ),
        };
        Ok(Self {
            state: wire.state,
            observations,
            sources: wire.sources,
        })
    }
}

impl ProcessResourceSnapshot {
    #[must_use]
    pub fn from_observations(
        state: DeviceState,
        observations: ProcessResourceObservations,
        sources: Vec<SourceStatus>,
    ) -> Self {
        Self {
            state,
            observations,
            sources,
        }
    }

    #[must_use]
    pub const fn state(&self) -> DeviceState {
        self.state
    }

    #[must_use]
    pub const fn observations(&self) -> &ProcessResourceObservations {
        &self.observations
    }

    #[must_use]
    pub fn sources(&self) -> &[SourceStatus] {
        &self.sources
    }

    #[must_use]
    pub fn current_limits(&self) -> Option<&[ResourceLimit]> {
        current_list_value(&self.observations.limits)
    }

    #[must_use]
    pub fn current_resource_groups(&self) -> Option<&[ResourceGroupMembership]> {
        current_list_value(&self.observations.resource_groups)
    }

    #[must_use]
    pub const fn current_memory_usage_bytes(&self) -> Option<u64> {
        current_copy_value(&self.observations.memory_usage_bytes)
    }

    #[must_use]
    pub const fn current_memory_limit(&self) -> Option<LimitValue> {
        current_copy_value(&self.observations.memory_limit)
    }

    #[must_use]
    pub const fn current_cpu_time_quota_micros(&self) -> Option<LimitValue> {
        current_copy_value(&self.observations.cpu_time_quota_micros)
    }

    #[must_use]
    pub const fn current_cpu_time_period_micros(&self) -> Option<u64> {
        current_copy_value(&self.observations.cpu_time_period_micros)
    }

    #[must_use]
    pub const fn current_process_count(&self) -> Option<u64> {
        current_copy_value(&self.observations.process_count)
    }

    #[must_use]
    pub const fn current_process_limit(&self) -> Option<LimitValue> {
        current_copy_value(&self.observations.process_limit)
    }

    /// Replace the canonical resource truth without exposing a mutable field.
    pub fn apply_observations(&mut self, observations: ProcessResourceObservations) {
        self.observations = observations;
    }

    /// Replace coarse/source health derived by the platform from the same
    /// canonical observation assembly.
    pub fn apply_source_truth(&mut self, state: DeviceState, sources: Vec<SourceStatus>) {
        self.state = state;
        self.sources = sources;
    }

    /// Retain independent limit and membership truth for the same frozen
    /// process. Group-derived fields additionally require a current, unchanged
    /// resource-group identity.
    #[must_use]
    pub fn retain_previous(mut self, previous: Self, same_resource_groups: bool) -> Self {
        let previous_observations = previous.observations;
        let current_observations = std::mem::take(&mut self.observations);
        self.observations =
            current_observations.retain_previous(previous_observations, same_resource_groups);
        self
    }
}

fn current_list_value<T>(observation: &ResourceObservation<Vec<T>>) -> Option<&[T]> {
    match observation {
        ResourceObservation::Unknown => None,
        ResourceObservation::Current { value, .. } | ResourceObservation::Partial { value, .. } => {
            Some(value.as_slice())
        }
        ResourceObservation::Absent { .. } => Some(&[]),
        ResourceObservation::Stale { .. } | ResourceObservation::Unavailable { .. } => None,
    }
}

const fn current_copy_value<T: Copy>(observation: &ResourceObservation<T>) -> Option<T> {
    match observation {
        ResourceObservation::Unknown => None,
        ResourceObservation::Current { value, .. } | ResourceObservation::Partial { value, .. } => {
            Some(*value)
        }
        ResourceObservation::Absent { .. }
        | ResourceObservation::Stale { .. }
        | ResourceObservation::Unavailable { .. } => None,
    }
}

fn hydrate_legacy_value<T>(
    observation: ResourceObservation<T>,
    legacy: Option<T>,
    last_success_ms: Option<u64>,
) -> ResourceObservation<T> {
    match (observation, legacy, last_success_ms) {
        (ResourceObservation::Unknown, Some(value), Some(observed_at_ms)) => {
            ResourceObservation::current(value, observed_at_ms)
        }
        (observation, _, _) => observation,
    }
}

fn hydrate_legacy_list<T>(
    observation: ResourceObservation<Vec<T>>,
    legacy: Vec<T>,
    last_success_ms: Option<u64>,
) -> ResourceObservation<Vec<T>> {
    let legacy = (!legacy.is_empty()).then_some(legacy);
    hydrate_legacy_value(observation, legacy, last_success_ms)
}

fn wire_current_list<T: Clone>(observation: &ResourceObservation<Vec<T>>) -> Vec<T> {
    match observation {
        ResourceObservation::Current { value, .. } => value.clone(),
        ResourceObservation::Unknown
        | ResourceObservation::Partial { .. }
        | ResourceObservation::Absent { .. }
        | ResourceObservation::Stale { .. }
        | ResourceObservation::Unavailable { .. } => Vec::new(),
    }
}

const fn wire_current_copy<T: Copy>(observation: &ResourceObservation<T>) -> Option<T> {
    match observation {
        ResourceObservation::Current { value, .. } => Some(*value),
        ResourceObservation::Unknown
        | ResourceObservation::Partial { .. }
        | ResourceObservation::Absent { .. }
        | ResourceObservation::Stale { .. }
        | ResourceObservation::Unavailable { .. } => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_telemetry_resources_tests.rs"]
mod tests;
