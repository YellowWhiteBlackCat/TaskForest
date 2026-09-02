//! Typed refresh requests and data delivered back to a frontend.

use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::services::{ServiceDeps, ServiceLogSnapshot, ServiceLogStreamSnapshot};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::RequestId;

use crate::ServiceControlOutcome;

/// A typed asynchronous service result. Keeping these in the shared update
/// stream lets every frontend consume the same provider-neutral lifecycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceUpdate {
    Action(ServiceControlOutcome),
    Dependencies {
        request_id: RequestId,
        service_id: ServiceId,
        deps: ServiceDeps,
    },
    DependenciesUnavailable {
        request_id: RequestId,
        service_id: ServiceId,
        error: FailureKind,
    },
    Logs {
        request_id: RequestId,
        snapshot: ServiceLogSnapshot,
    },
    LogStream {
        request_id: RequestId,
        observed_at_ms: u64,
        snapshot: ServiceLogStreamSnapshot,
    },
}

/// A refresh scope understood by application and platform adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RefreshRequest {
    /// Refresh the data needed by the visible dashboard. Detail pages and
    /// their low-frequency providers are loaded only when requested.
    Dashboard,
    All,
    Telemetry,
    HardwareInventory,
    Processes,
    PlatformLists,
    Services,
    Startup,
    Sessions,
    Health,
    /// Targeted power/battery refresh (capacity, charge rate, state per battery).
    /// Distinct from [`Health`](Self::Health) so a frontend can poll battery on
    /// its own cadence without re-fetching sensors/SMART.
    Power,
    Containers,
}
