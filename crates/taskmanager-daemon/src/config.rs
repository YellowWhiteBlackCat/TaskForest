//! Daemon runtime configuration, paths, and sampling intervals.

use std::path::PathBuf;

use taskmanager_ipc::IpcEndpoint;

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
            history_dir: default_daemon_history_dir(),
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

/// Platform-appropriate default storage location for system daemon history.
fn default_daemon_history_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(program_data) = std::env::var("ProgramData") {
            return PathBuf::from(program_data)
                .join("TaskForest")
                .join("history");
        }
        PathBuf::from(r"C:\ProgramData\TaskForest\history")
    }

    #[cfg(not(windows))]
    {
        // System-level default on Linux/Unix
        PathBuf::from("/var/lib/taskforest/history")
    }
}
