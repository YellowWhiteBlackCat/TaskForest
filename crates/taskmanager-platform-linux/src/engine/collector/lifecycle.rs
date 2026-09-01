//! Shared lifecycle reconciliation for hot-pluggable collector devices.

use std::collections::{HashMap, HashSet};

use taskmanager_core::core::device_state::{
    DeviceLifecycleDelta, DeviceLifecycleRegistry, DeviceRefreshOutcome, DeviceState,
};
use taskmanager_core::core::metrics::{DiskMetrics, GpuMetrics, NetworkMetrics};
use taskmanager_core::{DeviceGeneration, DeviceId};

pub(super) trait LifecycleMetric {
    fn stable_id(&self) -> &str;
    fn state(&self) -> DeviceState;
    fn set_state(&mut self, state: DeviceState);
    fn set_generation(&mut self, generation: DeviceGeneration);
}

impl LifecycleMetric for DiskMetrics {
    fn stable_id(&self) -> &str {
        &self.device_id
    }

    fn state(&self) -> DeviceState {
        self.device_state
    }

    fn set_state(&mut self, state: DeviceState) {
        self.device_state = state;
    }

    fn set_generation(&mut self, generation: DeviceGeneration) {
        self.device_generation = generation;
    }
}

impl LifecycleMetric for NetworkMetrics {
    fn stable_id(&self) -> &str {
        &self.device_id
    }

    fn state(&self) -> DeviceState {
        self.device_state
    }

    fn set_state(&mut self, state: DeviceState) {
        self.device_state = state;
    }

    fn set_generation(&mut self, generation: DeviceGeneration) {
        self.device_generation = generation;
    }
}

impl LifecycleMetric for GpuMetrics {
    fn stable_id(&self) -> &str {
        &self.device_id
    }

    fn state(&self) -> DeviceState {
        self.device_state
    }

    fn set_state(&mut self, state: DeviceState) {
        self.device_state = state;
    }

    fn set_generation(&mut self, generation: DeviceGeneration) {
        self.device_generation = generation;
    }
}

pub(super) fn reconcile_devices<T: LifecycleMetric>(
    registry: &mut DeviceLifecycleRegistry,
    devices: &mut [T],
    outcome: DeviceRefreshOutcome,
    now_ms: u64,
) -> DeviceLifecycleDelta {
    let discovered_devices = devices
        .iter()
        .filter(|device| !device.stable_id().is_empty())
        .map(|device| DeviceId::new(device.stable_id()))
        .collect::<Vec<_>>();
    reconcile_discovered_devices(registry, devices, &discovered_devices, outcome, now_ms)
}

/// Reconcile a read model that may retain cached rows after discovery failed.
///
/// Only identities explicitly enumerated during this refresh are observed as
/// present. Retained rows receive the registry's unavailable/absent state
/// after the refresh closes and cannot accidentally resurrect themselves.
pub(super) fn reconcile_discovered_devices<T: LifecycleMetric>(
    registry: &mut DeviceLifecycleRegistry,
    devices: &mut [T],
    discovered_devices: &[DeviceId],
    outcome: DeviceRefreshOutcome,
    now_ms: u64,
) -> DeviceLifecycleDelta {
    let discovered = discovered_devices
        .iter()
        .map(DeviceId::as_str)
        .collect::<HashSet<_>>();
    registry.begin_refresh();
    for device in &mut *devices {
        let stable_id = device.stable_id().to_owned();
        if stable_id.is_empty() || !discovered.contains(stable_id.as_str()) {
            continue;
        }
        registry.observe(stable_id, device.state(), now_ms);
    }
    let delta = registry.finish_refresh(outcome, now_ms);
    for device in devices {
        let Some(lifecycle) = registry.get(device.stable_id()) else {
            continue;
        };
        device.set_state(lifecycle.state);
        device.set_generation(lifecycle.generation);
    }
    delta
}

pub(super) fn prune_map<T>(map: &mut HashMap<String, T>, expired: &[DeviceId]) {
    if expired.is_empty() {
        return;
    }
    let expired: HashSet<&str> = expired.iter().map(DeviceId::as_str).collect();
    map.retain(|id, _| !expired.contains(id.as_str()));
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_collector_lifecycle_tests.rs"]
mod tests;
