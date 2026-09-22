//! CPU thermal-throttle trigger-counter capability port and events (the
//! CpuThrottle request lane).
//!
//! The periodic CPU projection already carries the same per-package counters
//! (`CpuPackageMetrics::{package,core}_throttle_count`); this lane is the
//! on-demand, capability-addressed answer for the `telemetry.cpu.throttle`
//! identity. It is produced by the same read and the same aggregation rule, so
//! a frontend that renders the counters from either path sees one fact.
//! One request = exactly one bounded sysfs read; request pacing belongs to the
//! frontend.

use taskmanager_core::CpuThrottleSnapshot;
use taskmanager_platform_contract::{CapabilityId, RequestPort};

/// One CPU thermal-throttle trigger-counter read for the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuThrottleRequest {
    Refresh,
}

bind_request_capability!(CpuThrottleRequest, CapabilityId::TELEMETRY_CPU_THROTTLE);

/// One bounded publication answering a [`CpuThrottleRequest`]. The snapshot
/// carries real per-package counters (a counter this host does not expose stays
/// absent on its row) or a typed failure — never a fabricated zero.
#[derive(Clone, Debug)]
pub enum CpuThrottleEvent {
    Update(CpuThrottleSnapshot),
}

impl CpuThrottleEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::TELEMETRY_CPU_THROTTLE
    }
}

pub type CpuThrottleRequestPort = dyn RequestPort<Request = CpuThrottleRequest>;

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_facets_cpu_throttle_tests.rs"]
mod tests;
