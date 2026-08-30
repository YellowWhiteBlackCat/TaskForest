//! SMBIOS memory inventory capability port and events (the MemorySmbios
//! request lane).
//!
//! The request is frontend-paced and user-initiated (the escalation discipline
//! forbids auto-triggering the OS-native prompt): a frontend submits one read
//! while a memory surface is visible. The provider performs ONE bounded
//! privileged SMBIOS helper invocation per request and answers with exactly one
//! [`SmbiosMemoryEvent::Update`] — real slot/module rows plus the system/board
//! identity facts on success, a typed failure (denied / helper unavailable /
//! unsupported) otherwise. This lane is system-scoped: unlike the per-device
//! engine-rows lane there is no device correlation field to echo, so the
//! request is a unit `Refresh` payload mirroring
//! [`super::system::HardwareInventoryRequest`].

use taskmanager_core::SmbiosMemorySnapshot;
use taskmanager_platform_contract::{CapabilityId, RequestPort};

/// One SMBIOS memory-inventory read for the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmbiosMemoryRequest {
    Refresh,
}

bind_request_capability!(SmbiosMemoryRequest, CapabilityId::TELEMETRY_MEMORY_SMBIOS);

/// One bounded publication answering a [`SmbiosMemoryRequest`]. The snapshot
/// carries either real slot/module rows plus identity facts or a typed
/// failure — never a fabricated inventory.
#[derive(Clone, Debug)]
pub enum SmbiosMemoryEvent {
    Update(SmbiosMemorySnapshot),
}

impl SmbiosMemoryEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::TELEMETRY_MEMORY_SMBIOS
    }
}

pub type SmbiosMemoryRequestPort = dyn RequestPort<Request = SmbiosMemoryRequest>;

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_facets_smbios_memory_tests.rs"]
mod tests;
