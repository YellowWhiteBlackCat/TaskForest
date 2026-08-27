//! Per-engine GPU utilization capability ports and events (the PMU lane).
//!
//! The request is frontend-paced and user-initiated (the escalation discipline
//! forbids auto-triggering the OS-native prompt): a frontend submits one read
//! for the device it is rendering while that surface is visible. The provider
//! performs ONE bounded PMU helper invocation per request and answers with
//! exactly one [`GpuEngineRowsEvent::Update`] — real rows on success, a typed
//! failure (denied / helper unavailable / unsupported) otherwise.

use taskmanager_core::{DeviceId, GpuEngineRowsSnapshot};
use taskmanager_platform_contract::{CapabilityId, RequestPort};

/// One per-engine read for `device_id`. The frontend chooses the device (it
/// knows which card it is rendering); the provider echoes the identity in the
/// snapshot so consumers can route rows to the right device surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuEngineRowsRequest {
    pub device_id: DeviceId,
}

bind_request_capability!(GpuEngineRowsRequest, CapabilityId::TELEMETRY_GPU_ENGINES);

/// One bounded publication answering a [`GpuEngineRowsRequest`]. The snapshot
/// carries either real engine rows or a typed failure — never a fabricated
/// zero-valued row.
#[derive(Clone, Debug)]
pub enum GpuEngineRowsEvent {
    Update(GpuEngineRowsSnapshot),
}

impl GpuEngineRowsEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::TELEMETRY_GPU_ENGINES
    }
}

pub type GpuEngineRowsRequestPort = dyn RequestPort<Request = GpuEngineRowsRequest>;

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_facets_gpu_engine_rows_tests.rs"]
mod tests;
