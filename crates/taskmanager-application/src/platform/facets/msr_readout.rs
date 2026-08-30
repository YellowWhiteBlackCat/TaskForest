//! CPU MSR-readout capability port and events (the CpuMsr request lane).
//!
//! The request is frontend-paced and user-initiated (the escalation discipline
//! forbids auto-triggering the OS-native prompt): a frontend submits one read
//! while the CPU details surface is visible. The provider performs ONE bounded
//! MSR helper invocation per request and answers with exactly one
//! [`MsrReadoutEvent::Update`] — real per-node register rows on success, a
//! typed failure (denied / helper unavailable / unsupported) otherwise. This
//! lane is system-scoped: the request is a unit `Refresh` payload mirroring
//! [`super::rapl_power::RaplPowerRequest`].

use taskmanager_core::MsrReadoutSnapshot;
use taskmanager_platform_contract::{CapabilityId, RequestPort};

/// One CPU MSR-readout read for the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsrReadoutRequest {
    Refresh,
}

bind_request_capability!(MsrReadoutRequest, CapabilityId::TELEMETRY_CPU_MSR);

/// One bounded publication answering a [`MsrReadoutRequest`]. The snapshot
/// carries either real per-node register rows or a typed failure — never a
/// fabricated zero for a register the CPU does not implement.
#[derive(Clone, Debug)]
pub enum MsrReadoutEvent {
    Update(MsrReadoutSnapshot),
}

impl MsrReadoutEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::TELEMETRY_CPU_MSR
    }
}

pub type MsrReadoutRequestPort = dyn RequestPort<Request = MsrReadoutRequest>;

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_facets_msr_readout_tests.rs"]
mod tests;
