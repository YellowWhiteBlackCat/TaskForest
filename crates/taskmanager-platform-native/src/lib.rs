#![forbid(unsafe_code)]

//! Compile-time selection of the native operating-system adapter.
//!
//! Frontends depend on this composition edge instead of naming an OS crate.
//! Each OS artifact includes its complete standard hardware-provider set; GPU,
//! storage, sensor, and transport implementations are selected and merged at
//! runtime rather than becoming product build variants.

use std::path::PathBuf;

#[cfg(all(not(debug_assertions), not(feature = "hardware-all")))]
compile_error!(
    "release builds require `hardware-all`; hardware backends are runtime-selected per OS"
);

#[cfg(target_os = "linux")]
pub use taskmanager_platform_linux::NativePlatformRuntime;

#[cfg(target_os = "linux")]
pub fn native_config_path() -> PathBuf {
    taskmanager_platform_linux::user_config_path()
}

#[cfg(target_os = "linux")]
pub fn native_history_dir() -> PathBuf {
    taskmanager_platform_linux::user_history_dir()
}

/// Discover validated local-time rules through the selected Linux adapter.
#[cfg(target_os = "linux")]
#[must_use]
pub fn native_local_time_rules() -> taskmanager_core::LocalTimeRulesObservation {
    taskmanager_platform_linux::local_time_rules()
}

/// Discover validated local-time rules through the Windows adapter: the
/// audited native boundary reports per-year zone rules, the adapter
/// synthesizes a TZif payload, and the core parser's validation is the only
/// acceptance gate.
#[cfg(target_os = "windows")]
#[must_use]
pub fn native_local_time_rules() -> taskmanager_core::LocalTimeRulesObservation {
    taskmanager_platform_windows::local_time_rules()
}

/// macOS keeps this capability explicit until its audited native time-zone
/// adapter exists. A frontend must render unavailable, never treat UTC as
/// though it were the user's local zone.
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
#[must_use]
pub fn native_local_time_rules() -> taskmanager_core::LocalTimeRulesObservation {
    taskmanager_core::LocalTimeRulesObservation::unsupported(taskmanager_core::unix_millis(
        std::time::SystemTime::now(),
    ))
}

/// Probe whether a process that held a native persistent-history lock is gone.
///
/// Linux delegates to its procfs adapter. Other targets conservatively decline
/// stale-lock reclamation until their native process-identity adapters provide
/// an equally strong answer.
#[cfg(target_os = "linux")]
#[must_use]
pub fn history_lock_holder_is_gone(pid: u32) -> bool {
    taskmanager_platform_linux::history_lock_holder_is_gone(pid)
}

#[cfg(not(target_os = "linux"))]
#[must_use]
pub fn history_lock_holder_is_gone(pid: u32) -> bool {
    let _ = pid;
    false
}

#[cfg(target_os = "macos")]
pub use taskmanager_platform_macos::NativePlatformRuntime;

#[cfg(target_os = "macos")]
pub fn native_config_path() -> PathBuf {
    taskmanager_platform_macos::user_config_path()
}

#[cfg(target_os = "macos")]
pub fn native_history_dir() -> PathBuf {
    taskmanager_platform_macos::user_history_dir()
}

#[cfg(target_os = "windows")]
pub use taskmanager_platform_windows::NativePlatformRuntime;

#[cfg(target_os = "windows")]
pub fn native_config_path() -> PathBuf {
    taskmanager_platform_windows::user_config_path()
}

#[cfg(target_os = "windows")]
pub fn native_history_dir() -> PathBuf {
    taskmanager_platform_windows::user_history_dir()
}

#[cfg(target_os = "windows")]
pub fn native_locale_name() -> Option<String> {
    taskmanager_platform_windows::user_locale_name()
}

#[cfg(not(target_os = "windows"))]
#[must_use]
pub fn native_locale_name() -> Option<String> {
    None
}

pub mod tray;

pub use tray::spawn_tray;

pub mod instance;

pub use instance::acquire_single_instance;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!(
    "this source tree does not yet contain a native adapter for the selected operating system"
);
