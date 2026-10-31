//! NPU inventory incarnation tracking for the application lifecycle owner.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use taskforest_core::core::device_state::{
    DEFAULT_DEVICE_ABSENCE_RETENTION_MS, DeviceLifecycle, DeviceLifecycleRegistry,
    DeviceRefreshOutcome, DeviceState, DeviceStatus,
};
use taskforest_core::core::identity::{DeviceGeneration, DeviceId};
use taskforest_core::core::npu::NpuInventorySnapshot;

use super::{
    DeviceLifecycleApplyResult, DeviceLifecyclePartition, DeviceLifecycleProjection,
    DeviceLifecycleProjectionIssue, DeviceLifecycleSnapshotRevision, issue_stable_id,
};

impl Default for DeviceLifecycleProjection {
    fn default() -> Self {
        Self {
            partitions: BTreeMap::new(),
            owners: BTreeMap::new(),
            retired_owner_order: Default::default(),
            missing_unconfirmed_order: Default::default(),
            devices: BTreeMap::new(),
            npu_registry: DeviceLifecycleRegistry::new(DEFAULT_DEVICE_ABSENCE_RETENTION_MS),
        }
    }
}

impl DeviceLifecycleProjection {
    /// Attach an incarnation generation before shell stores an NPU snapshot.
    /// Missing or repeated IDs make a successful response incomplete, so it
    /// cannot confirm absence for devices omitted from that response.
    pub fn apply_npu_inventory_snapshot(
        &mut self,
        revision: DeviceLifecycleSnapshotRevision,
        snapshot: &mut NpuInventorySnapshot,
    ) -> DeviceLifecycleApplyResult {
        let mut registry = self.npu_registry.clone();
        registry.begin_refresh();
        let mut id_counts = BTreeMap::<&str, usize>::new();
        let mut has_empty_id = false;
        for device in &snapshot.devices {
            if device.device_id.as_str().is_empty() {
                has_empty_id = true;
            } else {
                *id_counts.entry(device.device_id.as_str()).or_default() += 1;
            }
        }
        let duplicate_ids: BTreeSet<String> = id_counts
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(id, _)| id.to_owned())
            .collect();
        let outcome = match snapshot.failure.as_ref() {
            None => {
                for device in &snapshot.devices {
                    if !device.device_id.as_str().is_empty()
                        && !duplicate_ids.contains(device.device_id.as_str())
                    {
                        registry.observe(
                            device.device_id.clone(),
                            DeviceState::healthy(snapshot.observed_at_ms),
                            snapshot.observed_at_ms,
                        );
                    }
                }
                if duplicate_ids.is_empty() && !has_empty_id {
                    DeviceRefreshOutcome::Complete
                } else {
                    DeviceRefreshOutcome::Unavailable(DeviceStatus::Stale)
                }
            }
            Some(failure) => {
                DeviceRefreshOutcome::Unavailable(DeviceStatus::from_failure(failure.kind))
            }
        };
        let _ = registry.finish_refresh(outcome, snapshot.observed_at_ms);

        let sidecar = registry
            .iter()
            .map(|(id, lifecycle)| (id.to_owned(), *lifecycle))
            .collect::<HashMap<String, DeviceLifecycle>>();
        let mut result = self.apply_partition_sidecar(
            DeviceLifecyclePartition::NpuInventory,
            revision,
            &sidecar,
        );
        if let DeviceLifecycleApplyResult::Applied(delta) = &mut result {
            if has_empty_id {
                delta
                    .issues
                    .push(DeviceLifecycleProjectionIssue::EmptyStableId);
            }
            delta.issues.extend(duplicate_ids.iter().map(|id| {
                DeviceLifecycleProjectionIssue::DuplicateStableId {
                    stable_id: DeviceId::new(id.clone()),
                }
            }));
            delta
                .issues
                .sort_by(|left, right| issue_stable_id(left).cmp(issue_stable_id(right)));
            let conflicts: BTreeSet<&str> = delta
                .issues
                .iter()
                .filter_map(|issue| match issue {
                    DeviceLifecycleProjectionIssue::OwnershipConflict {
                        stable_id,
                        observed_partition: DeviceLifecyclePartition::NpuInventory,
                        ..
                    } => Some(stable_id.as_str()),
                    _ => None,
                })
                .collect();
            let duplicate_ids: BTreeSet<&str> = delta
                .issues
                .iter()
                .filter_map(|issue| match issue {
                    DeviceLifecycleProjectionIssue::DuplicateStableId { stable_id } => {
                        Some(stable_id.as_str())
                    }
                    _ => None,
                })
                .collect();
            let failed_inventory = snapshot.failure.is_some();
            for device in &mut snapshot.devices {
                if failed_inventory
                    || device.device_id.as_str().is_empty()
                    || duplicate_ids.contains(device.device_id.as_str())
                    || conflicts.contains(device.device_id.as_str())
                {
                    device.device_generation = DeviceGeneration::default();
                } else {
                    device.device_generation = registry
                        .get(device.device_id.as_str())
                        .map_or_else(DeviceGeneration::default, |lifecycle| lifecycle.generation);
                }
            }
            self.npu_registry = registry;
        }
        result
    }
}
