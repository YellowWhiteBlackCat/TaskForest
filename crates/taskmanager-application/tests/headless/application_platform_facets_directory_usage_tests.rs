use taskmanager_core::DirectoryScanBounds;
use taskmanager_platform_contract::{
    CapabilityRequest, MAX_REQUEST_SCOPE_BYTES, RequestTracking, RequestTrackingError,
};

use super::*;

#[test]
fn directory_usage_request_owns_the_filesystem_directory_usage_capability() {
    assert_eq!(
        DirectoryUsageRequest::CAPABILITY,
        CapabilityId::DIRECTORY_USAGE
    );
}

#[test]
fn scan_owns_a_root_scope_while_cancel_borrows_the_scan_lifecycle() {
    let start = DirectoryUsageRequest::StartScan(DirectoryScanSpec {
        root: "/data".to_string(),
        bounds: DirectoryScanBounds::default(),
    });
    assert!(matches!(
        start.runtime_tracking(),
        Ok(RequestTracking::Target(scope)) if scope.as_str() == "/data"
    ));
    assert_eq!(
        DirectoryUsageRequest::Cancel(DirectoryScanId::new(7)).runtime_tracking(),
        Ok(RequestTracking::Sideband)
    );
}

#[test]
fn scan_root_scope_rejects_oversized_identity_at_the_application_boundary() {
    let request = DirectoryUsageRequest::StartScan(DirectoryScanSpec {
        root: "a".repeat(MAX_REQUEST_SCOPE_BYTES + 1),
        bounds: DirectoryScanBounds::default(),
    });

    assert_eq!(
        request.runtime_tracking(),
        Err(RequestTrackingError::TargetScopeTooLong {
            actual_bytes: MAX_REQUEST_SCOPE_BYTES + 1,
            max_bytes: MAX_REQUEST_SCOPE_BYTES,
        })
    );
}

#[test]
fn update_events_only_accept_the_directory_usage_capability() {
    let update = DirectoryUsageEvent::Update(DirectoryUsageSnapshot {
        scan_id: DirectoryScanId::new(1),
        root: "/data".to_string(),
        status: taskmanager_core::DirectoryScanStatus::Scanning,
        entries: Vec::new(),
        totals: taskmanager_core::DirectoryScanTotals::fresh(10),
    });
    assert!(update.accepts_capability(&CapabilityId::DIRECTORY_USAGE));
    assert!(!update.accepts_capability(&CapabilityId::STORAGE_HEALTH));
}
