//! Generation-bound retention for independently fallible disk scalars.

use std::collections::{HashMap, HashSet};

use taskmanager_core::{DeviceGeneration, DeviceId, DiskMetrics, DiskScalarObservations};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DiskScalarKey {
    device_id: String,
    generation: DeviceGeneration,
}

#[derive(Debug, Default)]
pub(crate) struct DiskScalarState {
    observations: HashMap<DiskScalarKey, DiskScalarObservations>,
}

impl DiskScalarState {
    /// Retain failures only from the same stable identity and confirmed
    /// lifecycle generation. A reattached device can never inherit values
    /// from the hardware instance that occupied its prior generation.
    pub(crate) fn reconcile(&mut self, disks: &mut [DiskMetrics]) {
        for disk in disks {
            if disk.device_id.is_empty() || disk.device_generation.get() == 0 {
                continue;
            }
            let key = DiskScalarKey {
                device_id: disk.device_id.clone(),
                generation: disk.device_generation,
            };
            let observations = self
                .observations
                .get(&key)
                .copied()
                .map_or(*disk.scalar_observations(), |previous| {
                    disk.scalar_observations().retain_previous(previous)
                });
            self.observations.insert(key, observations);
            disk.apply_scalar_observations(observations);
        }
    }

    pub(crate) fn reset_generations(&mut self, device_ids: &[DeviceId]) {
        let ids = device_ids
            .iter()
            .map(DeviceId::as_str)
            .collect::<HashSet<_>>();
        self.observations
            .retain(|key, _| !ids.contains(key.device_id.as_str()));
    }

    pub(crate) fn expire(&mut self, device_ids: &[DeviceId]) {
        self.reset_generations(device_ids);
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_collector_disks_scalars_tests.rs"]
mod tests;
