//! Registered-pending CPU MSR-readout provider (macOS side of the CpuMsr
//! request lane).

use taskmanager_core::MsrReadoutSnapshot;
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::MsrReadoutProvider;

/// Registered-pending CPU MSR-readout provider: the lane is backed on Linux
/// by the MSR helper crossing through the 0600 `/dev/cpu/N/msr` nodes, which
/// do not exist on macOS (Apple silicon exposes no model-specific-register
/// file ABI), so the capability publishes an honest `Unsupported` descriptor
/// and every read completes with a typed failure — never a fabricated
/// register value (G-05 style, ADR-019).
pub struct PendingMsrReadoutProvider;

impl MsrReadoutProvider for PendingMsrReadoutProvider {
    fn read_msr_readouts(&mut self) -> Result<MsrReadoutSnapshot, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}
