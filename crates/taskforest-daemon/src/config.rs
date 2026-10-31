//! Daemon runtime configuration, paths, and sampling intervals.

use std::path::PathBuf;

use taskforest_ipc::IpcEndpoint;
use taskmanager_platform_native::{native_history_dir, native_system_daemon_history_dir};

/// Runtime configuration for the background daemon service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonConfig {
    /// IPC endpoint where the daemon listens for frontend client connections.
    pub endpoint: IpcEndpoint,
    /// Storage directory for continuous background history files.
    pub history_dir: PathBuf,
    /// Active sampling interval when at least one client subscribes (milliseconds).
    pub active_interval_ms: u32,
    /// Background idle sampling interval when zero clients are connected (milliseconds).
    pub idle_interval_ms: u32,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            endpoint: IpcEndpoint::default_platform(),
            history_dir: default_daemon_history_dir(false),
            active_interval_ms: 1000,
            idle_interval_ms: 10_000,
        }
    }
}

impl DaemonConfig {
    #[must_use]
    pub fn new(endpoint: IpcEndpoint, history_dir: impl Into<PathBuf>) -> Self {
        Self {
            endpoint,
            history_dir: history_dir.into(),
            ..Default::default()
        }
    }
}

/// Platform-appropriate default storage location for background daemon history.
#[must_use]
pub fn default_daemon_history_dir(is_user_mode: bool) -> PathBuf {
    if is_user_mode {
        native_history_dir()
    } else {
        native_system_daemon_history_dir()
    }
}
