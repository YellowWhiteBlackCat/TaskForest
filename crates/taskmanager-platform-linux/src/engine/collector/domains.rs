//! Independently callable Linux system-telemetry domain collectors.
//!
//! These collectors are the physical side of ADR-011. Each value owns only
//! its domain's I/O, rate baselines, caches, and lifecycle state. There is
//! deliberately no `observe_all` method: provider/runtime lanes schedule one
//! domain without waiting for any sibling.

use std::collections::BTreeMap;
use std::time::Instant;

use taskmanager_core::{
    DeviceId, DeviceLifecycle, DeviceStatus, FailureKind, ProviderId, ProviderRuntimeState,
    SourceOutcome, SourceStatus,
};

mod cpu;
mod gpu;
mod host;
mod memory;
mod network;
mod storage;

pub(crate) use cpu::LinuxCpuTelemetryCollector;
pub(crate) use gpu::LinuxGpuTelemetryCollector;
pub(crate) use host::LinuxHostTelemetryCollector;
pub(crate) use memory::LinuxMemoryTelemetryCollector;
pub(crate) use network::LinuxNetworkTelemetryCollector;
pub(crate) use storage::LinuxStorageTelemetryCollector;

/// Uniform synchronous boundary used by the six independent provider lanes.
///
/// Wall-clock milliseconds are supplied by the lane so fixture tests and
/// application revision ordering do not depend on the collector reading a
/// second clock.
pub(crate) trait LinuxSystemDomainCollector {
    type Observation;

    fn observe(&mut self, now: Instant, now_ms: u64) -> Self::Observation;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceQuality {
    Current,
    Partial(FailureKind),
    Unavailable(FailureKind),
}

fn source_quality(sources: &[SourceStatus]) -> SourceQuality {
    let mut has_success = false;
    let mut failure = None;
    for source in sources {
        match source.outcome {
            SourceOutcome::Available | SourceOutcome::Empty => has_success = true,
            SourceOutcome::Partial(candidate) => {
                has_success = true;
                failure = Some(stronger_failure(failure, candidate));
            }
            SourceOutcome::Unavailable(candidate) => {
                failure = Some(stronger_failure(failure, candidate));
            }
        }
    }
    match (has_success, failure) {
        (true, Some(failure)) => SourceQuality::Partial(failure),
        (true, None) => SourceQuality::Current,
        (false, Some(failure)) => SourceQuality::Unavailable(failure),
        (false, None) => SourceQuality::Unavailable(FailureKind::ProviderFault),
    }
}

fn device_quality(
    discovery: SourceOutcome,
    discovered_any: bool,
    sources: &[SourceStatus],
) -> SourceQuality {
    match discovery {
        SourceOutcome::Available | SourceOutcome::Empty => source_quality(sources),
        SourceOutcome::Partial(discovery_failure)
        | SourceOutcome::Unavailable(discovery_failure)
            if discovered_any =>
        {
            match source_quality(sources) {
                SourceQuality::Partial(failure) | SourceQuality::Unavailable(failure) => {
                    SourceQuality::Partial(stronger_failure(Some(discovery_failure), failure))
                }
                SourceQuality::Current => SourceQuality::Partial(discovery_failure),
            }
        }
        SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => {
            SourceQuality::Unavailable(failure)
        }
    }
}

const fn stronger_failure(current: Option<FailureKind>, candidate: FailureKind) -> FailureKind {
    match current {
        Some(current) if failure_priority(current) >= failure_priority(candidate) => current,
        Some(_) | None => candidate,
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::TimedOut => 7,
        FailureKind::IdentityChanged => 6,
        FailureKind::TemporarilyUnavailable => 5,
        FailureKind::MissingDependency => 4,
        FailureKind::Rejected => 3,
        FailureKind::ProviderFault => 2,
        FailureKind::Unsupported => 1,
    }
}

#[derive(Debug, Default)]
struct ProviderStateTracker {
    last_success: BTreeMap<ProviderId, u64>,
}

impl ProviderStateTracker {
    fn observe(&mut self, sources: &[SourceStatus], now_ms: u64) -> Vec<ProviderRuntimeState> {
        let current_providers = sources
            .iter()
            .map(|source| source.provider.clone())
            .collect::<Vec<_>>();
        self.last_success
            .retain(|provider, _| current_providers.contains(provider));
        sources
            .iter()
            .map(|source| {
                let status = match source.outcome {
                    SourceOutcome::Available | SourceOutcome::Empty => DeviceStatus::Healthy,
                    SourceOutcome::Partial(failure) => {
                        self.last_success.insert(source.provider.clone(), now_ms);
                        DeviceStatus::from_failure(failure)
                    }
                    SourceOutcome::Unavailable(failure) => DeviceStatus::from_failure(failure),
                };
                if matches!(
                    source.outcome,
                    SourceOutcome::Available | SourceOutcome::Empty
                ) {
                    self.last_success.insert(source.provider.clone(), now_ms);
                }
                ProviderRuntimeState {
                    provider: source.provider.clone(),
                    status,
                    last_success_ms: self.last_success.get(&source.provider).copied(),
                }
            })
            .collect()
    }
}

fn lifecycle_snapshot(
    registry: &taskmanager_core::DeviceLifecycleRegistry,
) -> BTreeMap<DeviceId, DeviceLifecycle> {
    registry
        .iter()
        .map(|(id, lifecycle)| (DeviceId::new(id), *lifecycle))
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_collector_domains_tests.rs"]
mod tests;
