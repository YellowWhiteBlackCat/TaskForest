//! Registered-pending SMBIOS memory-inventory provider (Windows side of the
//! MemorySmbios request lane).

use taskmanager_core::SmbiosMemorySnapshot;
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::SmbiosMemoryProvider;

/// Registered-pending SMBIOS memory-inventory provider: the lane is backed on
/// Linux by the polkit/pkexec helper crossing; the Windows equivalent would
/// need a typed WMI/SMBIOS seam that is not implemented yet, so the capability
/// publishes an honest `Unsupported` descriptor and every read completes with
/// a typed failure — never a fabricated inventory row (G-05 style, ADR-019).
pub struct PendingSmbiosMemoryProvider;

impl SmbiosMemoryProvider for PendingSmbiosMemoryProvider {
    fn read_memory_smbios(&mut self) -> Result<SmbiosMemorySnapshot, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}
