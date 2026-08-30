//! Registered-pending CPU package-power provider (macOS side of the
//! PackagePowerRapl request lane).

use taskmanager_core::RaplPowerSnapshot;
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::RaplPowerProvider;

/// Registered-pending CPU package-power provider: the lane is backed on Linux
/// by the RAPL sysfs helper crossing, which does not exist on macOS (Apple
/// silicon power regions need the private `powermetrics`/IOReport seam), so
/// the capability publishes an honest `Unsupported` descriptor and every read
/// completes with a typed failure — never a fabricated watt figure (G-05
/// style, ADR-019).
pub struct PendingRaplPowerProvider;

impl RaplPowerProvider for PendingRaplPowerProvider {
    fn read_package_power(&mut self) -> Result<RaplPowerSnapshot, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}
