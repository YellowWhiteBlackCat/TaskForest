use std::sync::Arc;

use taskmanager_core::core::sensors::SensorCenterSnapshot;
use taskmanager_platform_contract::{CapabilityId, DeviceSourceSnapshot, RequestPort};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SensorRequest {
    Refresh,
}

bind_request_capability!(SensorRequest, CapabilityId::SENSORS);

#[derive(Clone, Debug)]
pub enum SensorEvent {
    Snapshot(DeviceSourceSnapshot<SensorCenterSnapshot>),
}

impl SensorEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::SENSORS
    }
}

pub type SensorRequestPort = dyn RequestPort<Request = SensorRequest>;

/// Independently optional sensor capability ports.
#[derive(Clone, Default)]
pub struct SensorFacets {
    observation: Option<Arc<SensorRequestPort>>,
}

impl SensorFacets {
    #[must_use]
    pub fn with_observation(mut self, port: Arc<SensorRequestPort>) -> Self {
        self.observation = Some(port);
        self
    }

    #[must_use]
    pub fn observation(&self) -> Option<&SensorRequestPort> {
        self.observation.as_deref()
    }
}
