//! Storage system-telemetry domain collector.
//!
//! Owns `LinuxStorageTelemetryCollector`, which holds disk discovery, SMART,
//! rates, lifecycle, and generation-bound command-target publication.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use sysinfo::Disks;
use taskmanager_core::{
    DEFAULT_DEVICE_ABSENCE_RETENTION_MS, DeviceId, DeviceLifecycleRegistry, DeviceRefreshOutcome,
    DiskMetrics, StorageTelemetryObservation,
};

use super::{
    LinuxSystemDomainCollector, ProviderStateTracker, SourceQuality, device_quality,
    lifecycle_snapshot,
};
use crate::engine::collector::DiskCollectionState;
use crate::engine::collector::disks::{DiskScalarState, collect_storage_domain};
use crate::engine::collector::lifecycle::{prune_map, reconcile_discovered_devices};
use crate::engine::storage_target::{
    StorageTargetRegistry, StorageTargetResolver, storage_identity_metadata_outcome,
};

/// Storage-only collector owning discovery, SMART, rates, lifecycle, and
/// generation-bound command-target publication.
pub(crate) struct LinuxStorageTelemetryCollector {
    disks: Disks,
    sysfs_root: PathBuf,
    tick: u32,
    state: DiskCollectionState,
    lifecycles: DeviceLifecycleRegistry,
    provider_states: ProviderStateTracker,
    scalar_state: DiskScalarState,
    last_value: Option<(Vec<DiskMetrics>, u64)>,
    target_registry: StorageTargetRegistry,
}

impl LinuxStorageTelemetryCollector {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::with_sysfs_root(PathBuf::from("/sys/class/block"))
    }

    fn with_sysfs_root(sysfs_root: PathBuf) -> Self {
        Self {
            disks: Disks::new(),
            sysfs_root,
            tick: 0,
            state: DiskCollectionState::new(),
            lifecycles: DeviceLifecycleRegistry::new(DEFAULT_DEVICE_ABSENCE_RETENTION_MS),
            provider_states: ProviderStateTracker::default(),
            scalar_state: DiskScalarState::default(),
            last_value: None,
            target_registry: StorageTargetRegistry::new(),
        }
    }

    #[must_use]
    pub(crate) fn target_resolver(&self) -> StorageTargetResolver {
        self.target_registry.resolver()
    }

    pub(crate) fn observe(&mut self, now: Instant, now_ms: u64) -> StorageTelemetryObservation {
        <Self as LinuxSystemDomainCollector>::observe(self, now, now_ms)
    }

    fn reset_absent_devices(&mut self, device_ids: &[DeviceId]) {
        let absent_ids = device_ids.iter().collect::<HashSet<_>>();
        let absent_names = self
            .state
            .identity_cache
            .iter()
            .filter_map(|(name, id)| absent_ids.contains(id).then_some(name.clone()))
            .collect::<HashSet<_>>();
        self.state
            .stats
            .retain(|name, _| !absent_names.contains(name));
        prune_map(&mut self.state.smart_cache, device_ids);
        prune_map(&mut self.state.smart_source_cache, device_ids);
    }

    fn expire_devices(&mut self, device_ids: &[DeviceId]) {
        self.reset_absent_devices(device_ids);
        self.scalar_state.expire(device_ids);
        let expired_ids = device_ids.iter().collect::<HashSet<_>>();
        self.state
            .identity_cache
            .retain(|_, id| !expired_ids.contains(id));
    }
}

impl Default for LinuxStorageTelemetryCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSystemDomainCollector for LinuxStorageTelemetryCollector {
    type Observation = StorageTelemetryObservation;

    fn observe(&mut self, now: Instant, now_ms: u64) -> Self::Observation {
        self.disks.refresh(true);
        self.tick = self.tick.wrapping_add(1);
        let snapshot = collect_storage_domain(
            &self.disks,
            Path::new(&self.sysfs_root),
            &mut self.state,
            self.tick,
            now,
            now_ms,
        );
        let discovery = snapshot.discovery().clone();
        let discovered_devices = snapshot.discovered_devices().to_vec();
        let refresh = DeviceRefreshOutcome::from_discovery_outcome(discovery.outcome);
        let mut metrics = snapshot.value;
        let lifecycle_delta = reconcile_discovered_devices(
            &mut self.lifecycles,
            &mut metrics,
            &discovered_devices,
            refresh,
            now_ms,
        );
        self.reset_absent_devices(&lifecycle_delta.newly_absent);
        self.scalar_state
            .reset_generations(&lifecycle_delta.reappeared);
        self.scalar_state.reconcile(&mut metrics);
        for disk in &mut metrics {
            for partition in &mut disk.partitions {
                partition.parent_device_id = disk.device_id.clone();
                partition.device_generation = disk.device_generation;
            }
        }
        self.expire_devices(&lifecycle_delta.expired);

        self.target_registry.publish(
            &metrics,
            discovery.outcome,
            storage_identity_metadata_outcome(&snapshot.enrichments),
        );
        let mut sources = Vec::with_capacity(snapshot.enrichments.len().saturating_add(1));
        sources.push(discovery.clone());
        sources.extend(snapshot.enrichments);
        let provider_states = self.provider_states.observe(&sources, now_ms);
        let lifecycles = lifecycle_snapshot(&self.lifecycles);

        let quality = device_quality(discovery.outcome, !discovered_devices.is_empty(), &sources);
        match quality {
            SourceQuality::Current => {
                self.last_value = Some((metrics.clone(), now_ms));
                StorageTelemetryObservation::current(
                    metrics,
                    now_ms,
                    sources,
                    provider_states,
                    lifecycles,
                )
            }
            SourceQuality::Partial(failure) => {
                self.last_value = Some((metrics.clone(), now_ms));
                StorageTelemetryObservation::partial(
                    metrics,
                    now_ms,
                    failure,
                    sources,
                    provider_states,
                    lifecycles,
                )
            }
            SourceQuality::Unavailable(failure) => self.last_value.as_ref().map_or_else(
                || {
                    StorageTelemetryObservation::unavailable(
                        failure,
                        sources.clone(),
                        provider_states.clone(),
                        lifecycles.clone(),
                    )
                },
                |(last_value, last_success_ms)| {
                    StorageTelemetryObservation::stale(
                        last_value.clone(),
                        *last_success_ms,
                        failure,
                        sources.clone(),
                        provider_states.clone(),
                        lifecycles.clone(),
                    )
                },
            ),
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_collector_domains_storage_tests.rs"]
mod tests;
