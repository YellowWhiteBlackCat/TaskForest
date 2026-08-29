//! Toolkit-neutral system-tray seam.
//!
//! The tray is a process-lifetime object hosted by the frontend, not a
//! worker-lane capability, so it deliberately bypasses the request/port
//! machinery: the frontend builds a validated [`TraySpec`], calls
//! [`spawn_tray`], and receives user interactions as [`TrayEvent`]s on a
//! channel it owns. Each OS adapter owns the native object and the thread
//! rules that platform imposes (Linux StatusNotifierItem and the Windows
//! tray icon on a dedicated host thread, macOS `NSStatusItem` on the
//! application's main thread).

#![forbid(unsafe_code)]

use std::sync::mpsc::Sender;

use taskmanager_core::core::tray::{TrayEvent, TraySpec};
use taskmanager_platform_contract::{TrayController, TrayFailure};

/// Spawn the system tray for the given validated spec.
///
/// Event delivery: the tray's interactions arrive on `events` from whatever
/// thread the OS adapter uses (the host thread on Linux/Windows, the calling
/// thread on macOS); the sender is shared so the frontend polls it from its
/// event loop.
///
/// Dropping the returned controller removes the tray from the system (with a
/// best-effort host-thread join on Linux/Windows).
pub fn spawn_tray(
    spec: TraySpec,
    events: Sender<TrayEvent>,
) -> Result<Box<dyn TrayController>, TrayFailure> {
    #[cfg(target_os = "linux")]
    {
        taskmanager_platform_linux::tray::spawn_tray(spec, events)
    }
    #[cfg(target_os = "macos")]
    {
        taskmanager_platform_macos::tray::spawn_tray(spec, events)
    }
    #[cfg(target_os = "windows")]
    {
        taskmanager_platform_windows::tray::spawn_tray(spec, events)
    }
}
