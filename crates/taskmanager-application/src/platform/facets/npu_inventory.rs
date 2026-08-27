//! NPU accelerator inventory capability port and events.
//!
//! Frontend-paced request/response lane (discovery-first):
//! a frontend submits one inventory read while an accelerator surface is
//! visible; the provider answers with exactly one
//! [`NpuInventoryEvent::Update`] — a sorted device list on success (an empty
//! list is the honest no-NPU host) or a typed failure otherwise. Live
//! utilization stays a typed observation inside each device and is
//! `Unavailable(Unsupported)` until a stable kernel interface exists.

use taskmanager_core::NpuInventorySnapshot;
use taskmanager_platform_contract::{CapabilityId, RequestPort};

/// One accelerator inventory read for the host.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct NpuInventoryRequest {}

bind_request_capability!(NpuInventoryRequest, CapabilityId::ACCELERATOR_NPU);

/// One bounded publication answering a [`NpuInventoryRequest`].
#[derive(Clone, Debug)]
pub enum NpuInventoryEvent {
    Update(NpuInventorySnapshot),
}

impl NpuInventoryEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::ACCELERATOR_NPU
    }
}

pub type NpuInventoryRequestPort = dyn RequestPort<Request = NpuInventoryRequest>;

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_facets_npu_inventory_tests.rs"]
mod tests;
