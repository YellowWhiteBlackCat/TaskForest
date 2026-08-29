//! Capability-absent platform handle construction.
//!
//! This is the honest starting point for a native adapter that has not yet
//! implemented any provider. It publishes no capability descriptors, installs
//! no request ports, and emits no fabricated domain events.

use std::sync::Arc;

use taskmanager_application::{PlatformEvent, PlatformFacets, PlatformHandle};
use taskmanager_platform_contract::{
    CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
};

#[derive(Default)]
struct AbsentCapabilityCatalog;

impl CapabilityCatalog for AbsentCapabilityCatalog {
    fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot::default()
    }
}

#[derive(Default)]
struct IdleEventPort;

impl EventPort for IdleEventPort {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        Ok(None)
    }
}

/// Construct a live handle whose capability set is empty.
///
/// An adapter may use this while its real provider registry is empty. The
/// application rejects submissions through missing facets as
/// `UnsupportedCapability`; the idle event port remains live and therefore
/// does not misreport an intentional absence as `RuntimeStopped`.
#[must_use]
pub fn capability_absent_handle() -> PlatformHandle {
    PlatformHandle::new(
        Arc::new(AbsentCapabilityCatalog),
        Arc::new(IdleEventPort),
        PlatformFacets::default(),
    )
}

#[cfg(test)]
#[path = "../tests/headless/runtime_absent_tests.rs"]
mod tests;
