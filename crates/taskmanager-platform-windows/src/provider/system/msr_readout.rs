//! Registered-pending CPU MSR-readout provider (Windows side of the CpuMsr
//! request lane).

use taskmanager_core::MsrReadoutSnapshot;
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::MsrReadoutProvider;

/// Registered-pending CPU MSR-readout provider: the lane is backed on Linux
/// by the MSR helper crossing through the root-only `/dev/cpu/N/msr` nodes;
/// the Windows equivalent would need a typed driver/`__readmsr` seam that is
/// not implemented yet, so the capability publishes an honest `Unsupported`
/// descriptor and every read completes with a typed failure — never a
/// fabricated register value (G-05 style, ADR-019).
pub struct PendingMsrReadoutProvider;

impl MsrReadoutProvider for PendingMsrReadoutProvider {
    fn read_msr_readouts(&mut self) -> Result<MsrReadoutSnapshot, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}
