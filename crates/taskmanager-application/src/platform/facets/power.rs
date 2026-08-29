use std::sync::Arc;

use taskmanager_core::core::power::PowerSupplySnapshot;
use taskmanager_platform_contract::{CapabilityId, DeviceSourceSnapshot, RequestPort};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerSupplyRequest {
    Refresh,
}

bind_request_capability!(PowerSupplyRequest, CapabilityId::POWER_SUPPLIES);

#[derive(Clone, Debug)]
pub enum PowerSupplyEvent {
    Snapshot(DeviceSourceSnapshot<PowerSupplySnapshot>),
}

impl PowerSupplyEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::POWER_SUPPLIES
    }
}

pub type PowerSupplyRequestPort = dyn RequestPort<Request = PowerSupplyRequest>;

/// Independently optional power capability ports.
#[derive(Clone, Default)]
pub struct PowerFacets {
    supplies: Option<Arc<PowerSupplyRequestPort>>,
}

impl PowerFacets {
    #[must_use]
    pub fn with_supplies(mut self, port: Arc<PowerSupplyRequestPort>) -> Self {
        self.supplies = Some(port);
        self
    }

    #[must_use]
    pub fn supplies(&self) -> Option<&PowerSupplyRequestPort> {
        self.supplies.as_deref()
    }
}
