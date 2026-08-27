//! Network system-telemetry domain collector.
//!
//! Owns `LinuxNetworkTelemetryCollector`, which holds adapter counters, rate
//! baselines, metadata, and per-device lifecycle for network interfaces only.

use std::time::Instant;

use sysinfo::Networks;
use taskmanager_core::{
    DEFAULT_DEVICE_ABSENCE_RETENTION_MS, DeviceLifecycleRegistry, DeviceRefreshOutcome,
    NetworkMetrics, NetworkTelemetryObservation,
};

use super::{
    LinuxSystemDomainCollector, ProviderStateTracker, SourceQuality, device_quality,
    lifecycle_snapshot,
};
use crate::engine::collector::lifecycle::reconcile_discovered_devices;
use crate::engine::collector::network::{
    NetworkCollectionState, NetworkDomainSnapshot, collect_network_domain,
};

type NetworkDomainCollector =
    fn(&Networks, &mut NetworkCollectionState, Instant, u64) -> NetworkDomainSnapshot;

/// Network-only collector owning adapter counters, rate baselines, metadata,
/// and lifecycle. It never refreshes CPU, memory, storage, SMART, or GPU state.
pub(crate) struct LinuxNetworkTelemetryCollector {
    networks: Networks,
    state: NetworkCollectionState,
    lifecycles: DeviceLifecycleRegistry,
    provider_states: ProviderStateTracker,
    last_value: Option<(Vec<NetworkMetrics>, u64)>,
    collect_domain: NetworkDomainCollector,
}

impl LinuxNetworkTelemetryCollector {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::with_domain_collector(collect_network_domain)
    }

    fn with_domain_collector(collect_domain: NetworkDomainCollector) -> Self {
        Self {
            networks: Networks::new(),
            state: NetworkCollectionState::default(),
            lifecycles: DeviceLifecycleRegistry::new(DEFAULT_DEVICE_ABSENCE_RETENTION_MS),
            provider_states: ProviderStateTracker::default(),
            last_value: None,
            collect_domain,
        }
    }

    pub(crate) fn observe(&mut self, now: Instant, now_ms: u64) -> NetworkTelemetryObservation {
        <Self as LinuxSystemDomainCollector>::observe(self, now, now_ms)
    }
}

impl Default for LinuxNetworkTelemetryCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSystemDomainCollector for LinuxNetworkTelemetryCollector {
    type Observation = NetworkTelemetryObservation;

    fn observe(&mut self, now: Instant, now_ms: u64) -> Self::Observation {
        self.networks.refresh(true);
        let snapshot = (self.collect_domain)(&self.networks, &mut self.state, now, now_ms);
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
        self.state.reset_absent(&lifecycle_delta.newly_absent);
        self.state.confirm_reappeared(&lifecycle_delta.reappeared);
        self.state.expire(&lifecycle_delta.expired);

        let mut sources = Vec::with_capacity(snapshot.enrichments.len().saturating_add(1));
        sources.push(discovery.clone());
        sources.extend(snapshot.enrichments);
        let provider_states = self.provider_states.observe(&sources, now_ms);
        let lifecycles = lifecycle_snapshot(&self.lifecycles);
        let quality = device_quality(discovery.outcome, !discovered_devices.is_empty(), &sources);
        match quality {
            SourceQuality::Current => {
                self.last_value = Some((metrics.clone(), now_ms));
                NetworkTelemetryObservation::current(
                    metrics,
                    now_ms,
                    sources,
                    provider_states,
                    lifecycles,
                )
            }
            SourceQuality::Partial(failure) => {
                self.last_value = Some((metrics.clone(), now_ms));
                NetworkTelemetryObservation::partial(
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
                    NetworkTelemetryObservation::unavailable(
                        failure,
                        sources.clone(),
                        provider_states.clone(),
                        lifecycles.clone(),
                    )
                },
                |(last_value, last_success_ms)| {
                    NetworkTelemetryObservation::stale(
                        if metrics.is_empty() {
                            last_value.clone()
                        } else {
                            // Retained inventory rows carry the newest
                            // per-field stale/unavailable evidence. The outer
                            // domain is still stale, but diagnostics/export
                            // must not lose the exact field failure by
                            // replaying an older all-current payload.
                            metrics.clone()
                        },
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
#[path = "../../../../tests/headless/linux_engine_collector_domains_network_tests.rs"]
mod tests;
