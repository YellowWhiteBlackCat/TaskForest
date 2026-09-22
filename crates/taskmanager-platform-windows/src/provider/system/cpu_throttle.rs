//! Registered-pending CPU thermal-throttle counter provider (Windows side of
//! the CpuThrottle request lane).

use taskmanager_core::CpuThrottleSnapshot;
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::CpuThrottleProvider;

/// Registered-pending CPU thermal-throttle counter provider: the cumulative
/// `thermal_throttle` trigger counters are a Linux sysfs fact. Windows reports
/// instantaneous processor-power/throttle indicators through PDH and the
/// native performance APIs, but exposes no cumulative per-package event
/// counters with the same semantics, so the capability publishes an honest
/// `Unsupported` descriptor and every read completes with a typed failure —
/// never a fabricated zero (G-05 style, ADR-019).
pub struct PendingCpuThrottleProvider;

impl CpuThrottleProvider for PendingCpuThrottleProvider {
    fn read_cpu_throttle(&mut self) -> Result<CpuThrottleSnapshot, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}
