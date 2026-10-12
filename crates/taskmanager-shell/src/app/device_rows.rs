//! Renderer-neutral device-row identity, geometry anchors, and family revisions.
//!
//! Row identity comes from the owning core fact. Application lifecycle
//! authority admits an identity only when its owner partition and incarnation
//! generation still match; labels and positions never replace a missing ID.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::SystemProjectionStore;
mod identity;

pub use identity::{DeviceRowFamily, DeviceRowId};

/// Monotonic accepted owner-fact revision for a row family.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceRowDataRevision(u64);

impl DeviceRowDataRevision {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// Monotonic ordering revision, advanced only when stable row order changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceRowOrderRevision(u64);

impl DeviceRowOrderRevision {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// Current owner-data and row-order revisions for one family.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DeviceRowProjectionGeneration {
    family: DeviceRowFamily,
    data: DeviceRowDataRevision,
    order: DeviceRowOrderRevision,
}

impl DeviceRowProjectionGeneration {
    #[must_use]
    pub const fn family(self) -> DeviceRowFamily {
        self.family
    }

    #[must_use]
    pub const fn data_revision(self) -> DeviceRowDataRevision {
        self.data
    }

    #[must_use]
    pub const fn order_revision(self) -> DeviceRowOrderRevision {
        self.order
    }
}

/// Delayed row input tied to the family projection that produced its geometry.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeviceRowAnchor {
    id: DeviceRowId,
    generation: DeviceRowProjectionGeneration,
    index: usize,
}

impl DeviceRowAnchor {
    #[must_use]
    pub const fn id(&self) -> &DeviceRowId {
        &self.id
    }

    #[must_use]
    pub const fn generation(&self) -> DeviceRowProjectionGeneration {
        self.generation
    }

    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }
}

#[derive(Clone, Debug, Default)]
struct DeviceRowFamilyRevisionState {
    data: DeviceRowDataRevision,
    order: DeviceRowOrderRevision,
    ordered_identities: Vec<DeviceRowId>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DeviceRowRevisionState {
    families: BTreeMap<DeviceRowFamily, DeviceRowFamilyRevisionState>,
    pending_families: std::collections::BTreeSet<DeviceRowFamily>,
}

impl DeviceRowRevisionState {
    fn generation(&self, family: DeviceRowFamily) -> DeviceRowProjectionGeneration {
        let state = self.families.get(&family).cloned().unwrap_or_default();
        DeviceRowProjectionGeneration {
            family,
            data: state.data,
            order: state.order,
        }
    }
}

impl SystemProjectionStore {
    /// Stable row identities in the canonical owner order for one family.
    #[must_use]
    pub fn device_row_ids(&self, family: DeviceRowFamily) -> Vec<DeviceRowId> {
        let candidates = match family {
            DeviceRowFamily::Storage => self
                .snapshot
                .iter()
                .flat_map(|snapshot| {
                    snapshot.disks.iter().flat_map(|disk| {
                        std::iter::once(DeviceRowId::from_disk(disk)).chain(
                            disk.partitions
                                .iter()
                                .map(|partition| DeviceRowId::from_partition(disk, partition)),
                        )
                    })
                })
                .flatten()
                .collect(),
            DeviceRowFamily::Network => self
                .snapshot
                .iter()
                .flat_map(|snapshot| snapshot.networks.iter())
                .filter_map(DeviceRowId::from_network)
                .collect(),
            DeviceRowFamily::Gpu => self
                .snapshot
                .iter()
                .flat_map(|snapshot| snapshot.gpu.iter())
                .filter_map(DeviceRowId::from_gpu)
                .collect(),
            DeviceRowFamily::PowerSupply => self
                .power_supplies
                .iter()
                .flat_map(|snapshot| snapshot.batteries.iter())
                .filter_map(DeviceRowId::from_power_supply)
                .collect(),
            DeviceRowFamily::Sensor => self
                .sensors
                .iter()
                .flat_map(|snapshot| snapshot.readings.iter())
                .filter_map(DeviceRowId::from_sensor)
                .collect(),
            DeviceRowFamily::Npu => self
                .npu_inventory
                .iter()
                .filter(|snapshot| snapshot.failure.is_none())
                .flat_map(|snapshot| snapshot.devices.iter())
                .filter_map(DeviceRowId::from_npu)
                .collect(),
        };

        unique_identities(candidates)
            .into_iter()
            .filter(|identity| self.lifecycle_authorizes(identity))
            .collect()
    }

    /// Row identity at an index in the current family projection.
    #[must_use]
    pub fn device_row_identity_at(
        &self,
        family: DeviceRowFamily,
        index: usize,
    ) -> Option<DeviceRowId> {
        self.device_row_ids(family).get(index).cloned()
    }

    /// Current identity plus data/order revisions for an indexed row.
    #[must_use]
    pub fn device_row_anchor_at(
        &self,
        family: DeviceRowFamily,
        index: usize,
    ) -> Option<DeviceRowAnchor> {
        Some(DeviceRowAnchor {
            id: self.device_row_identity_at(family, index)?,
            generation: self.device_row_projection_generation(family),
            index,
        })
    }

    /// Resolve identity after a reorder, independently from any old geometry.
    #[must_use]
    pub fn reconcile_device_row_identity(&self, identity: &DeviceRowId) -> Option<usize> {
        self.device_row_ids(identity.family())
            .iter()
            .position(|current| current == identity)
    }

    /// Resolve an anchor only while its identity, data and order revisions are
    /// still the ones represented by the current projection.
    #[must_use]
    pub fn resolve_device_row_anchor(&self, anchor: &DeviceRowAnchor) -> Option<usize> {
        let family = anchor.generation.family();
        (anchor.generation == self.device_row_projection_generation(family)
            && self.device_row_identity_at(family, anchor.index) == Some(anchor.id.clone()))
        .then_some(anchor.index)
    }

    /// Current source-data and geometry revisions for a device family.
    #[must_use]
    pub fn device_row_projection_generation(
        &self,
        family: DeviceRowFamily,
    ) -> DeviceRowProjectionGeneration {
        self.device_row_revisions.generation(family)
    }

    pub(crate) fn note_device_row_family_changed(&mut self, family: DeviceRowFamily) {
        let identities = self.device_row_ids(family);
        let state = self
            .device_row_revisions
            .families
            .entry(family)
            .or_default();
        state.data = state.data.next();
        if state.ordered_identities != identities {
            state.order = state.order.next();
            state.ordered_identities = identities;
        }
    }

    pub(crate) fn queue_device_row_family_change(&mut self, family: DeviceRowFamily) {
        self.device_row_revisions.pending_families.insert(family);
    }

    pub(crate) fn commit_device_row_family_changes(&mut self) {
        let pending = std::mem::take(&mut self.device_row_revisions.pending_families);
        for family in pending {
            self.note_device_row_family_changed(family);
        }
    }

    fn lifecycle_authorizes(&self, identity: &DeviceRowId) -> bool {
        let (device_id, generation) = identity.physical_owner();
        let family = identity.family();
        self.device_lifecycle_projection
            .authority(device_id.as_str())
            == Some(family.lifecycle_partition())
            && self
                .device_lifecycle_projection
                .get(device_id.as_str())
                .is_some_and(|lifecycle| lifecycle.generation == generation)
    }
}

fn unique_identities(candidates: Vec<DeviceRowId>) -> Vec<DeviceRowId> {
    let mut counts = HashMap::new();
    for identity in &candidates {
        *counts.entry(identity.clone()).or_insert(0_usize) += 1;
    }
    let mut emitted = HashSet::new();
    candidates
        .into_iter()
        .filter(|identity| counts.get(identity) == Some(&1) && emitted.insert(identity.clone()))
        .collect()
}

#[cfg(test)]
#[path = "../../tests/headless/device_rows.rs"]
mod tests;
