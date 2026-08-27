use taskmanager_application::{CapabilityId, ProviderId, RequestId};

use crate::environment::PendingEnvironmentRuntimeLanes;
use crate::integration::PendingIntegrationRuntimeLanes;
use crate::power::PendingPowerRuntimeLanes;
use crate::process::PendingProcessRuntimeLanes;
use crate::sensor::PendingSensorRuntimeLanes;
use crate::service::PendingServiceRuntimeLanes;
use crate::storage::PendingStorageRuntimeLanes;
use crate::system::PendingSystemRuntimeLanes;

/// Correlation and provider attribution captured before a request enters a
/// provider execution lane.
pub struct Queued<R> {
    pub(crate) request_id: RequestId,
    pub(crate) capability: CapabilityId,
    pub(crate) provider: ProviderId,
    pub(crate) payload: R,
}

/// Typed provider-side receivers for every application capability.
///
/// Each present receiver is independently bounded; an absent provider binding
/// leaves the corresponding field `None`. Native adapters attach provider
/// closures with [`crate::spawn_lane`] or [`crate::spawn_typed_outcome_lane`].
pub struct RuntimeLanes {
    pub system: PendingSystemRuntimeLanes,
    pub process: PendingProcessRuntimeLanes,
    pub service: PendingServiceRuntimeLanes,
    pub environment: PendingEnvironmentRuntimeLanes,
    pub integration: PendingIntegrationRuntimeLanes,
    pub storage: PendingStorageRuntimeLanes,
    pub sensor: PendingSensorRuntimeLanes,
    pub power: PendingPowerRuntimeLanes,
}
