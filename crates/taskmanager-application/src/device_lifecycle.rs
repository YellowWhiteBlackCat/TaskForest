//! Toolkit-neutral projection of shared device lifecycle sidecars.
//!
//! Frontends consume this reducer instead of interpreting lifecycle sidecars
//! from system telemetry, sensor, or power-supply snapshots themselves.
//! Stable IDs remain opaque: no platform, vendor, bus, or device-family
//! vocabulary is inferred from their text.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use taskmanager_core::{
    DeviceGeneration, DeviceId, DeviceLifecycle, DevicePresence, DeviceState, DeviceStatus,
    GpuTelemetryObservation, NetworkTelemetryObservation, PowerSupplySnapshot,
    SensorCenterSnapshot, StorageTelemetryObservation,
};

/// Application-facing lifecycle state.
///
/// Discovery unavailability is deliberately separate from a confirmed
/// disconnection. `Removed` is represented by a transition after a retained
/// disconnected sidecar expires, not by a current row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceLifecycleViewState {
    Present,
    Degraded(DeviceStatus),
    ProviderUnavailable(DeviceStatus),
    Disconnected,
}

/// Authoritative lifecycle source within one application process.
///
/// These are capability partitions, not operating-system, vendor, bus, or
/// hardware-family labels. Stable IDs remain globally opaque.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeviceLifecyclePartition {
    SystemStorage,
    SystemNetwork,
    SystemGpu,
    Sensors,
    PowerSupplies,
}

/// One stable application read-model row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedDeviceLifecycle {
    pub partition: DeviceLifecyclePartition,
    pub stable_id: DeviceId,
    pub generation: DeviceGeneration,
    pub state: DeviceLifecycleViewState,
    pub health: DeviceState,
    pub first_seen_ms: Option<u64>,
    pub last_seen_ms: Option<u64>,
    pub disconnected_since_ms: Option<u64>,
}

/// Semantic transition emitted by the reducer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceLifecycleChangeKind {
    Discovered,
    HealthChanged,
    ProviderUnavailable,
    ProviderRecovered,
    Disconnected,
    Reappeared,
    Removed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceLifecycleChange {
    pub partition: DeviceLifecyclePartition,
    pub kind: DeviceLifecycleChangeKind,
    pub stable_id: DeviceId,
    pub previous_generation: Option<DeviceGeneration>,
    pub current_generation: Option<DeviceGeneration>,
    pub current: Option<ProjectedDeviceLifecycle>,
}

/// Non-fatal producer inconsistency retained for diagnostics.
///
/// The reducer never repairs these cases by inventing a lifecycle transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceLifecycleProjectionIssue {
    EmptyStableId,
    ZeroGeneration {
        stable_id: DeviceId,
    },
    GenerationRegressed {
        stable_id: DeviceId,
        retained: DeviceGeneration,
        observed: DeviceGeneration,
    },
    MissingWithoutConfirmedDisconnect {
        stable_id: DeviceId,
        retained_generation: DeviceGeneration,
    },
    OwnershipConflict {
        stable_id: DeviceId,
        authoritative_partition: DeviceLifecyclePartition,
        observed_partition: DeviceLifecyclePartition,
    },
}

/// Monotonic revision assigned at the application/event boundary.
///
/// This is intentionally not derived inside the reducer from a platform clock.
/// A caller may use an event sequence or another monotonic snapshot revision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceLifecycleSnapshotRevision(u64);

impl DeviceLifecycleSnapshotRevision {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceLifecycleSnapshotRejection {
    Duplicate {
        partition: DeviceLifecyclePartition,
        revision: DeviceLifecycleSnapshotRevision,
    },
    ConflictingDuplicate {
        partition: DeviceLifecyclePartition,
        revision: DeviceLifecycleSnapshotRevision,
    },
    OutOfOrder {
        partition: DeviceLifecyclePartition,
        accepted: DeviceLifecycleSnapshotRevision,
        received: DeviceLifecycleSnapshotRevision,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceLifecycleProjectionDelta {
    pub partition: DeviceLifecyclePartition,
    pub revision: DeviceLifecycleSnapshotRevision,
    pub changes: Vec<DeviceLifecycleChange>,
    pub issues: Vec<DeviceLifecycleProjectionIssue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceLifecycleApplyResult {
    Applied(DeviceLifecycleProjectionDelta),
    Ignored(DeviceLifecycleSnapshotRejection),
}

const DEVICE_LIFECYCLE_DIAGNOSTIC_CAPACITY: usize = 32;
const DEVICE_LIFECYCLE_OWNER_TOMBSTONE_CAPACITY: usize = 1_024;
/// Hard ceiling on rows retained in the "missing without a confirmed
/// disconnect" state. A provider that stops reporting a device without ever
/// confirming its disappearance cannot prove it is gone, so those rows are
/// retained — but only up to this ceiling: past it, a newly missing row is
/// retired through the same honest removal path as a confirmed disconnect
/// instead of growing the projection for the whole process life.
const DEVICE_LIFECYCLE_UNCONFIRMED_RETENTION_CAPACITY: usize = 1_024;

/// Bounded diagnostic tail shared by toolkit adapters.
///
/// Applied deltas retain non-fatal producer issues while ignored entries retain
/// the exact duplicate or ordering rejection. This state is diagnostic only and
/// never synthesizes device presence.
#[derive(Clone, Debug, Default)]
pub struct DeviceLifecycleDiagnosticHistory {
    entries: VecDeque<DeviceLifecycleApplyResult>,
}

impl DeviceLifecycleDiagnosticHistory {
    pub fn record(&mut self, result: DeviceLifecycleApplyResult) {
        if self.entries.len() == DEVICE_LIFECYCLE_DIAGNOSTIC_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(result);
    }

    #[must_use]
    pub fn latest(&self) -> Option<&DeviceLifecycleApplyResult> {
        self.entries.back()
    }

    pub fn entries(&self) -> impl ExactSizeIterator<Item = &DeviceLifecycleApplyResult> {
        self.entries.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Stateful, platform-neutral application projection.
#[derive(Clone, Debug, Default)]
pub struct DeviceLifecycleProjection {
    partitions: BTreeMap<DeviceLifecyclePartition, DeviceLifecyclePartitionState>,
    owners: BTreeMap<DeviceId, DeviceLifecyclePartition>,
    retired_owner_order: VecDeque<DeviceId>,
    /// Insertion order of rows currently retained as missing without a
    /// confirmed disconnect. Members leave the queue when the device
    /// reappears, is confirmed disconnected, or is retired at the ceiling.
    missing_unconfirmed_order: VecDeque<DeviceId>,
    devices: BTreeMap<DeviceId, ProjectedDeviceLifecycle>,
}

#[derive(Clone, Debug, Default)]
struct DeviceLifecyclePartitionState {
    accepted_revision: Option<DeviceLifecycleSnapshotRevision>,
    accepted_sidecar: BTreeMap<String, DeviceLifecycle>,
}

impl DeviceLifecycleProjection {
    #[must_use]
    pub fn accepted_revision_for(
        &self,
        partition: DeviceLifecyclePartition,
    ) -> Option<DeviceLifecycleSnapshotRevision> {
        self.partitions
            .get(&partition)
            .and_then(|state| state.accepted_revision)
    }

    #[must_use]
    pub fn authority(&self, stable_id: &str) -> Option<DeviceLifecyclePartition> {
        self.owners.get(stable_id).copied()
    }

    #[must_use]
    pub fn get(&self, stable_id: &str) -> Option<&ProjectedDeviceLifecycle> {
        self.devices.get(stable_id)
    }

    pub fn devices(&self) -> impl ExactSizeIterator<Item = &ProjectedDeviceLifecycle> {
        self.devices.values()
    }

    pub fn devices_in_partition(
        &self,
        partition: DeviceLifecyclePartition,
    ) -> impl Iterator<Item = &ProjectedDeviceLifecycle> {
        self.devices
            .values()
            .filter(move |device| device.partition == partition)
    }

    /// Apply storage lifecycle directly from its independently scheduled
    /// typed observation. No aggregate `SystemSnapshot` is fabricated.
    pub fn apply_storage_telemetry_observation(
        &mut self,
        revision: DeviceLifecycleSnapshotRevision,
        observation: &StorageTelemetryObservation,
    ) -> DeviceLifecycleApplyResult {
        self.apply_typed_system_sidecar(
            DeviceLifecyclePartition::SystemStorage,
            revision,
            observation.device_lifecycles(),
        )
    }

    /// Apply network lifecycle directly from its independently scheduled
    /// typed observation. Its revision is independent from storage and GPU.
    pub fn apply_network_telemetry_observation(
        &mut self,
        revision: DeviceLifecycleSnapshotRevision,
        observation: &NetworkTelemetryObservation,
    ) -> DeviceLifecycleApplyResult {
        self.apply_typed_system_sidecar(
            DeviceLifecyclePartition::SystemNetwork,
            revision,
            observation.device_lifecycles(),
        )
    }

    /// Apply GPU lifecycle directly from its runtime-selected provider
    /// observation without treating a sibling domain as a completion barrier.
    pub fn apply_gpu_telemetry_observation(
        &mut self,
        revision: DeviceLifecycleSnapshotRevision,
        observation: &GpuTelemetryObservation,
    ) -> DeviceLifecycleApplyResult {
        self.apply_typed_system_sidecar(
            DeviceLifecyclePartition::SystemGpu,
            revision,
            observation.device_lifecycles(),
        )
    }

    pub fn apply_sensor_snapshot(
        &mut self,
        revision: DeviceLifecycleSnapshotRevision,
        snapshot: &SensorCenterSnapshot,
    ) -> DeviceLifecycleApplyResult {
        self.apply_partition_sidecar(
            DeviceLifecyclePartition::Sensors,
            revision,
            &snapshot.device_lifecycles,
        )
    }

    pub fn apply_power_supply_snapshot(
        &mut self,
        revision: DeviceLifecycleSnapshotRevision,
        snapshot: &PowerSupplySnapshot,
    ) -> DeviceLifecycleApplyResult {
        self.apply_partition_sidecar(
            DeviceLifecyclePartition::PowerSupplies,
            revision,
            &snapshot.device_lifecycles,
        )
    }

    fn apply_typed_system_sidecar(
        &mut self,
        partition: DeviceLifecyclePartition,
        revision: DeviceLifecycleSnapshotRevision,
        sidecar: &BTreeMap<DeviceId, DeviceLifecycle>,
    ) -> DeviceLifecycleApplyResult {
        let sidecar = sidecar
            .iter()
            .map(|(id, lifecycle)| (id.as_str().to_owned(), *lifecycle))
            .collect::<HashMap<_, _>>();
        self.apply_partition_sidecar(partition, revision, &sidecar)
    }

    /// Apply a capability-owned sidecar without interpreting stable-ID text.
    fn apply_partition_sidecar(
        &mut self,
        partition: DeviceLifecyclePartition,
        revision: DeviceLifecycleSnapshotRevision,
        sidecar: &HashMap<String, DeviceLifecycle>,
    ) -> DeviceLifecycleApplyResult {
        let incoming = sidecar
            .iter()
            .map(|(stable_id, lifecycle)| (stable_id.clone(), *lifecycle))
            .collect::<BTreeMap<_, _>>();

        if let Some(accepted_state) = self.partitions.get(&partition)
            && let Some(accepted) = accepted_state.accepted_revision
        {
            if revision < accepted {
                return DeviceLifecycleApplyResult::Ignored(
                    DeviceLifecycleSnapshotRejection::OutOfOrder {
                        partition,
                        accepted,
                        received: revision,
                    },
                );
            }
            if revision == accepted {
                let rejection = if incoming == accepted_state.accepted_sidecar {
                    DeviceLifecycleSnapshotRejection::Duplicate {
                        partition,
                        revision,
                    }
                } else {
                    DeviceLifecycleSnapshotRejection::ConflictingDuplicate {
                        partition,
                        revision,
                    }
                };
                return DeviceLifecycleApplyResult::Ignored(rejection);
            }
        }

        let mut next = self.devices.clone();
        let mut changes = Vec::new();
        let mut issues = Vec::new();
        let mut supplied_ids = BTreeSet::new();

        for (stable_id, lifecycle) in &incoming {
            if stable_id.is_empty() {
                issues.push(DeviceLifecycleProjectionIssue::EmptyStableId);
                continue;
            }
            let stable_id = DeviceId::new(stable_id.clone());
            supplied_ids.insert(stable_id.clone());
            let observed_generation = lifecycle.generation;
            if !observed_generation.is_valid() {
                issues.push(DeviceLifecycleProjectionIssue::ZeroGeneration { stable_id });
                continue;
            }
            if let Some(authoritative_partition) = self.owners.get(stable_id.as_str()).copied() {
                if authoritative_partition != partition {
                    issues.push(DeviceLifecycleProjectionIssue::OwnershipConflict {
                        stable_id,
                        authoritative_partition,
                        observed_partition: partition,
                    });
                    continue;
                }
            } else {
                self.owners.insert(stable_id.clone(), partition);
            }
            self.retired_owner_order
                .retain(|retired| retired != &stable_id);
            self.missing_unconfirmed_order
                .retain(|missing| missing != &stable_id);

            let previous = self.devices.get(stable_id.as_str());
            if let Some(previous) = previous
                && observed_generation < previous.generation
            {
                issues.push(DeviceLifecycleProjectionIssue::GenerationRegressed {
                    stable_id,
                    retained: previous.generation,
                    observed: observed_generation,
                });
                continue;
            }

            let mut current = project(partition, stable_id.clone(), *lifecycle);
            if let Some(previous) = previous
                && previous.generation == current.generation
            {
                merge_monotonic_metadata(previous, &mut current);
            }
            if let Some(kind) = classify_change(previous, &current) {
                changes.push(DeviceLifecycleChange {
                    partition,
                    kind,
                    stable_id: stable_id.clone(),
                    previous_generation: previous.map(|value| value.generation),
                    current_generation: Some(current.generation),
                    current: Some(current.clone()),
                });
            }
            next.insert(stable_id, current);
        }

        for (stable_id, previous) in &self.devices {
            if previous.partition != partition || supplied_ids.contains(stable_id) {
                continue;
            }
            if previous.state == DeviceLifecycleViewState::Disconnected {
                next.remove(stable_id);
                self.missing_unconfirmed_order
                    .retain(|missing| missing != stable_id);
                // Keep a bounded ownership tombstone so a recently removed
                // device cannot have its opaque ID immediately hijacked by a
                // different capability partition. The tail is cardinality
                // bounded; oldest confirmed removals eventually release their
                // owner instead of accumulating for the whole process life.
                if !self.retired_owner_order.contains(stable_id) {
                    self.retired_owner_order.push_back(stable_id.clone());
                }
                changes.push(DeviceLifecycleChange {
                    partition,
                    kind: DeviceLifecycleChangeKind::Removed,
                    stable_id: stable_id.clone(),
                    previous_generation: Some(previous.generation),
                    current_generation: None,
                    current: None,
                });
                continue;
            }
            let issue = DeviceLifecycleProjectionIssue::MissingWithoutConfirmedDisconnect {
                stable_id: stable_id.clone(),
                retained_generation: previous.generation,
            };
            let already_retained = self.missing_unconfirmed_order.contains(stable_id);
            if already_retained
                || self.missing_unconfirmed_order.len()
                    < DEVICE_LIFECYCLE_UNCONFIRMED_RETENTION_CAPACITY
            {
                if !already_retained {
                    self.missing_unconfirmed_order.push_back(stable_id.clone());
                }
                issues.push(issue);
            } else {
                // The unconfirmed-retention ceiling is full: retire this
                // newcomer through the same honest removal path as a
                // confirmed disconnect. Present devices are never affected —
                // the ceiling only governs how long absent-and-unconfirmed
                // rows may stay retained.
                next.remove(stable_id);
                if !self.retired_owner_order.contains(stable_id) {
                    self.retired_owner_order.push_back(stable_id.clone());
                }
                changes.push(DeviceLifecycleChange {
                    partition,
                    kind: DeviceLifecycleChangeKind::Removed,
                    stable_id: stable_id.clone(),
                    previous_generation: Some(previous.generation),
                    current_generation: None,
                    current: None,
                });
                issues.push(issue);
            }
        }

        while self.retired_owner_order.len() > DEVICE_LIFECYCLE_OWNER_TOMBSTONE_CAPACITY {
            let Some(expired) = self.retired_owner_order.pop_front() else {
                break;
            };
            if !next.contains_key(expired.as_str()) {
                self.owners.remove(expired.as_str());
            }
        }

        changes.sort_by(|left, right| left.stable_id.cmp(&right.stable_id));
        issues.sort_by(|left, right| issue_stable_id(left).cmp(issue_stable_id(right)));
        self.partitions.insert(
            partition,
            DeviceLifecyclePartitionState {
                accepted_revision: Some(revision),
                accepted_sidecar: incoming,
            },
        );
        self.devices = next;
        DeviceLifecycleApplyResult::Applied(DeviceLifecycleProjectionDelta {
            partition,
            revision,
            changes,
            issues,
        })
    }
}

fn project(
    partition: DeviceLifecyclePartition,
    stable_id: DeviceId,
    lifecycle: DeviceLifecycle,
) -> ProjectedDeviceLifecycle {
    let state = match lifecycle.presence {
        DevicePresence::Present if lifecycle.state.status == DeviceStatus::Healthy => {
            DeviceLifecycleViewState::Present
        }
        DevicePresence::Present => DeviceLifecycleViewState::Degraded(lifecycle.state.status),
        DevicePresence::Unavailable => {
            DeviceLifecycleViewState::ProviderUnavailable(lifecycle.state.status)
        }
        DevicePresence::Absent => DeviceLifecycleViewState::Disconnected,
    };
    ProjectedDeviceLifecycle {
        partition,
        stable_id,
        generation: lifecycle.generation,
        state,
        health: lifecycle.state,
        first_seen_ms: lifecycle.first_seen_ms,
        last_seen_ms: lifecycle.last_seen_ms,
        disconnected_since_ms: lifecycle.absent_since_ms,
    }
}

fn merge_monotonic_metadata(
    previous: &ProjectedDeviceLifecycle,
    current: &mut ProjectedDeviceLifecycle,
) {
    current.health.last_success_ms = max_optional(
        previous.health.last_success_ms,
        current.health.last_success_ms,
    );
    current.first_seen_ms = min_optional(previous.first_seen_ms, current.first_seen_ms);
    current.last_seen_ms = max_optional(previous.last_seen_ms, current.last_seen_ms);
    if current.state == DeviceLifecycleViewState::Disconnected {
        current.disconnected_since_ms = min_optional(
            previous.disconnected_since_ms,
            current.disconnected_since_ms,
        );
    } else {
        current.disconnected_since_ms = None;
    }
}

fn classify_change(
    previous: Option<&ProjectedDeviceLifecycle>,
    current: &ProjectedDeviceLifecycle,
) -> Option<DeviceLifecycleChangeKind> {
    let Some(previous) = previous else {
        return Some(match current.state {
            DeviceLifecycleViewState::ProviderUnavailable(_) => {
                DeviceLifecycleChangeKind::ProviderUnavailable
            }
            DeviceLifecycleViewState::Disconnected => DeviceLifecycleChangeKind::Disconnected,
            DeviceLifecycleViewState::Present | DeviceLifecycleViewState::Degraded(_) => {
                DeviceLifecycleChangeKind::Discovered
            }
        });
    };

    if current.generation > previous.generation {
        return Some(match current.state {
            DeviceLifecycleViewState::ProviderUnavailable(_) => {
                DeviceLifecycleChangeKind::ProviderUnavailable
            }
            DeviceLifecycleViewState::Disconnected => DeviceLifecycleChangeKind::Disconnected,
            DeviceLifecycleViewState::Present | DeviceLifecycleViewState::Degraded(_) => {
                DeviceLifecycleChangeKind::Reappeared
            }
        });
    }
    match (previous.state, current.state) {
        (DeviceLifecycleViewState::Disconnected, DeviceLifecycleViewState::Present)
        | (DeviceLifecycleViewState::Disconnected, DeviceLifecycleViewState::Degraded(_)) => {
            Some(DeviceLifecycleChangeKind::Reappeared)
        }
        (_, DeviceLifecycleViewState::Disconnected)
            if previous.state != DeviceLifecycleViewState::Disconnected =>
        {
            Some(DeviceLifecycleChangeKind::Disconnected)
        }
        (_, DeviceLifecycleViewState::ProviderUnavailable(_))
            if !matches!(
                previous.state,
                DeviceLifecycleViewState::ProviderUnavailable(_)
            ) =>
        {
            Some(DeviceLifecycleChangeKind::ProviderUnavailable)
        }
        (DeviceLifecycleViewState::ProviderUnavailable(_), _)
            if !matches!(
                current.state,
                DeviceLifecycleViewState::ProviderUnavailable(_)
            ) =>
        {
            Some(DeviceLifecycleChangeKind::ProviderRecovered)
        }
        _ if previous.state != current.state => Some(DeviceLifecycleChangeKind::HealthChanged),
        _ => None,
    }
}

const fn max_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(if left > right { left } else { right }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

const fn min_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(if left < right { left } else { right }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn issue_stable_id(issue: &DeviceLifecycleProjectionIssue) -> &str {
    match issue {
        DeviceLifecycleProjectionIssue::EmptyStableId => "",
        DeviceLifecycleProjectionIssue::ZeroGeneration { stable_id }
        | DeviceLifecycleProjectionIssue::GenerationRegressed { stable_id, .. }
        | DeviceLifecycleProjectionIssue::MissingWithoutConfirmedDisconnect { stable_id, .. }
        | DeviceLifecycleProjectionIssue::OwnershipConflict { stable_id, .. } => stable_id.as_str(),
    }
}

#[cfg(test)]
#[path = "../tests/headless/device_lifecycle.rs"]
mod tests;
