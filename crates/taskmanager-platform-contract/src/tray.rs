//! Neutral system-tray seam types.
//!
//! The tray is a process-lifetime object hosted by the frontend, not a
//! worker-lane capability, so it deliberately bypasses the request/port
//! machinery: the frontend builds a validated [`TraySpec`] (from
//! `taskmanager-core`), calls the native adapter's spawn function, and
//! receives user interactions as [`TrayEvent`]s on a channel it owns.
//!
//! Implementations of [`TrayController`] must be `Send + Sync`:
//!
//! - Linux and Windows route every mutation to a dedicated host thread.
//! - macOS keeps the native `NSStatusItem` in a main-thread slot and refuses
//!   mutations from any other thread with a typed
//!   [`TrayFailure::WrongThread`]; spawning must happen on the application
//!   main thread.

#![forbid(unsafe_code)]

use taskmanager_core::tray::TrayActionId;

use crate::TrayFailure;

/// Runtime handle to one spawned tray.
pub trait TrayController: Send + Sync {
    /// Show or hide the tray icon. Linux SNI has no hide/show for a
    /// registered item, so the Linux adapter reports `Unsupported`.
    fn set_visible(&self, visible: bool) -> Result<(), TrayFailure>;

    /// Replace the hover tooltip. `None` removes it where the platform
    /// supports removal; otherwise it is ignored.
    fn set_tooltip(&self, tooltip: Option<String>) -> Result<(), TrayFailure>;

    /// Replace the StatusNotifierItem title (Linux/macOS). Windows has no
    /// tray title; the Windows adapter reports `Unsupported`.
    fn set_title(&self, title: Option<String>) -> Result<(), TrayFailure>;

    /// Update the checked state of a checkmark or radio item.
    fn set_item_checked(&self, id: TrayActionId, checked: bool) -> Result<(), TrayFailure>;
}
