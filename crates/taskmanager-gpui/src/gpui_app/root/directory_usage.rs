//! Directory-usage scan UI state on `RootView`.
//!
//! The scan lifecycle lives on the platform side: `StartScan` / `Cancel`
//! requests run on their own bounded lane and come back as
//! [`DirectoryUsageEvent`] publications in the next platform event batch.
//! RootView only owns the latest snapshot (the Disk page panel derives its
//! rendering from the typed [`DirectoryScanStatus`]) and the two submission
//! helpers. A superseded or stale scan can never clobber a newer snapshot:
//! scan ids are monotonically issued request ids, and the apply path rejects
//! older ids.

use taskmanager_application::DirectoryUsageRequest;

use crate::core::{
    DirectoryScanBounds, DirectoryScanId, DirectoryScanSpec, DirectoryScanStatus,
    DirectoryUsageSnapshot,
};
use crate::gpui_app::root::RootView;

impl RootView {
    /// Start (or resume — starting the same root again supersedes) a bounded
    /// scan. Returns `true` when the request was submitted. No platform
    /// (worker stopped, or test construction) is an honest no-op, never a
    /// fabricated scan.
    pub(crate) fn start_directory_scan(&mut self, root: String) -> bool {
        let Some(platform) = self.platform.as_mut() else {
            return false;
        };
        let spec = DirectoryScanSpec {
            root,
            bounds: DirectoryScanBounds::default(),
        };
        platform
            .submit_directory_usage(
                DirectoryUsageRequest::StartScan(spec),
                super::platform_submission_time_ms(),
            )
            .is_ok()
    }

    /// Cancel the currently-running scan, if any. Returns `true` when the
    /// request was submitted; cancelling with no active scan (or no platform)
    /// is an idempotent no-op per the request contract.
    pub(crate) fn cancel_directory_scan(&mut self) -> bool {
        let Some(scan_id) = active_scan_id(self.directory_usage()) else {
            return false;
        };
        let Some(platform) = self.platform.as_mut() else {
            return false;
        };
        platform
            .submit_directory_usage(
                DirectoryUsageRequest::Cancel(scan_id),
                super::platform_submission_time_ms(),
            )
            .is_ok()
    }
}

/// The scan id of the active (still `Scanning`) scan, if any. Terminal
/// snapshots keep their result on screen but are never cancellable.
#[must_use]
pub(crate) fn active_scan_id(current: Option<&DirectoryUsageSnapshot>) -> Option<DirectoryScanId> {
    match current {
        Some(snapshot) if snapshot.status == DirectoryScanStatus::Scanning => {
            Some(snapshot.scan_id)
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_directory_usage_tests.rs"]
mod tests;
