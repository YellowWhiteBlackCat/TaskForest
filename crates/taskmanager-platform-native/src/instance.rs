//! Neutral single-instance seam.
//!
//! The GUI product is single-instance (ADR-032 follow-up): a second launch
//! activates the existing instance's main window and exits. This seam is the
//! toolkit-neutral dispatch point; each OS adapter owns its mechanism
//! (Linux D-Bus well-known name, macOS per-user Unix socket, Windows named
//! mutex + named event — cores borrowed from `tauri-plugin-single-instance`).

#![forbid(unsafe_code)]

use std::sync::mpsc::Sender;

use taskmanager_platform_contract::{InstanceEvent, InstanceFailure, InstanceRole};

/// Acquire single-instance ownership.
///
/// `Primary(guard)`: this process owns the instance; hold `guard` for the
/// process lifetime. Incoming activation requests arrive on `events`.
/// `Secondary`: another instance exists; the adapter has already asked it to
/// show its window, so the caller should exit promptly.
pub fn acquire_single_instance(
    instance_name: &str,
    events: Sender<InstanceEvent>,
) -> Result<InstanceRole, InstanceFailure> {
    #[cfg(target_os = "linux")]
    {
        taskmanager_platform_linux::instance::acquire_single_instance(instance_name, events)
    }
    #[cfg(target_os = "macos")]
    {
        taskmanager_platform_macos::instance::acquire_single_instance(instance_name, events)
    }
    #[cfg(target_os = "windows")]
    {
        taskmanager_platform_windows::instance::acquire_single_instance(instance_name, events)
    }
}
