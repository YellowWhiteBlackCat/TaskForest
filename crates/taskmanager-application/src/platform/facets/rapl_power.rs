//! CPU package-power capability port and events (the PackagePowerRapl request
//! lane).
//!
//! The request is frontend-paced and user-initiated (the escalation discipline
//! forbids auto-triggering the OS-native prompt): a frontend submits one read
//! while a power surface is visible. The provider performs ONE bounded RAPL
//! helper sample per request and answers with exactly one
//! [`RaplPowerEvent::Update`] — real per-package watt figures on success, a
//! typed failure (denied / helper unavailable / unsupported) otherwise. This
//! lane is system-scoped: the request is a unit `Refresh` payload mirroring
//! [`super::system::HardwareInventoryRequest`].

use taskmanager_core::RaplPowerSnapshot;
use taskmanager_platform_contract::{CapabilityId, RequestPort};

/// One CPU package-power read for the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaplPowerRequest {
    Refresh,
}

bind_request_capability!(RaplPowerRequest, CapabilityId::TELEMETRY_CPU_PACKAGE_POWER);

/// One bounded publication answering a [`RaplPowerRequest`]. The snapshot
/// carries either real per-package watt figures or a typed failure — never a
/// fabricated zero-watt reading.
#[derive(Clone, Debug)]
pub enum RaplPowerEvent {
    Update(RaplPowerSnapshot),
}

impl RaplPowerEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::TELEMETRY_CPU_PACKAGE_POWER
    }
}

pub type RaplPowerRequestPort = dyn RequestPort<Request = RaplPowerRequest>;

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_facets_rapl_power_tests.rs"]
mod tests;
