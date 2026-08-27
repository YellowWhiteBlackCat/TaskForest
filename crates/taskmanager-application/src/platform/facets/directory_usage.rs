//! Directory-usage analysis capability ports and events.
//!
//! The request is a user-initiated scan lifecycle: start (which doubles as the
//! resume path — a fresh scan of the same root) and cancel. Progress and
//! terminal results arrive as bounded [`DirectoryUsageEvent::Update`]
//! publications on this capability's own lane; the UI lane never blocks on a
//! scan.

use taskmanager_core::{DirectoryScanId, DirectoryScanSpec, DirectoryUsageSnapshot};
use taskmanager_platform_contract::{
    CapabilityId, CapabilityRequest, RequestPort, RequestScope, RequestTracking,
    RequestTrackingError, SidebandPolicy,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirectoryUsageRequest {
    /// Start a bounded scan of `spec.root`. Starting a scan for the same root
    /// again is the resume path: a new scan id supersedes the previous one.
    StartScan(DirectoryScanSpec),
    /// Cancel the scan with the given id. Cancelling an unknown or already
    /// finished scan is acknowledged with no state change (idempotent).
    Cancel(DirectoryScanId),
}

impl CapabilityRequest for DirectoryUsageRequest {
    const CAPABILITY: CapabilityId = CapabilityId::DIRECTORY_USAGE;
    const SIDEBAND_POLICY: SidebandPolicy = SidebandPolicy::Idempotent;

    fn runtime_tracking(&self) -> Result<RequestTracking, RequestTrackingError> {
        match self {
            Self::StartScan(spec) => {
                RequestScope::try_from_str(&spec.root).map(RequestTracking::Target)
            }
            // Cancellation is folded into the addressed scan by the worker;
            // that scan's original request publishes the terminal lifecycle.
            Self::Cancel(_) => Ok(RequestTracking::Sideband),
        }
    }
}

/// One bounded publication for a directory-usage scan. `Scanning` snapshots
/// are progress; `Completed` / `Cancelled` / `Failed` are terminal.
#[derive(Clone, Debug)]
pub enum DirectoryUsageEvent {
    Update(DirectoryUsageSnapshot),
}

impl DirectoryUsageEvent {
    #[must_use]
    pub fn accepts_capability(&self, capability: &CapabilityId) -> bool {
        capability == &CapabilityId::DIRECTORY_USAGE
    }
}

pub type DirectoryUsageRequestPort = dyn RequestPort<Request = DirectoryUsageRequest>;

#[cfg(test)]
#[path = "../../../tests/headless/application_platform_facets_directory_usage_tests.rs"]
mod tests;
