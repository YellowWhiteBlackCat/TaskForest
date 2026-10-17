//! Cross-platform endpoint resolution for Unix Domain Sockets and Windows Named Pipes.

use std::env;
#[cfg(not(windows))]
use std::path::Path;
use std::path::PathBuf;

/// Cross-platform IPC endpoint designation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcEndpoint {
    /// Unix Domain Socket filesystem path (Linux / macOS).
    Unix(PathBuf),
    /// Windows Named Pipe identifier path (Windows).
    Windows(String),
}

impl IpcEndpoint {
    /// Default socket path on Unix-like operating systems.
    pub const DEFAULT_UNIX_PATH: &str = "/run/taskforest/daemon.sock";

    /// Default named pipe identifier on Windows operating systems.
    pub const DEFAULT_WINDOWS_PIPE: &str = r"\\.\pipe\taskforest-daemon";

    /// Environment variable overriding the daemon socket on Unix.
    pub const ENV_UNIX_SOCKET: &str = "TASKFOREST_DAEMON_SOCKET";

    /// Environment variable overriding the daemon named pipe on Windows.
    pub const ENV_WINDOWS_PIPE: &str = "TASKFOREST_DAEMON_PIPE";

    /// Resolve the default platform-native IPC endpoint for the current session.
    #[must_use]
    pub fn default_platform() -> Self {
        #[cfg(windows)]
        {
            if let Ok(custom) = env::var(Self::ENV_WINDOWS_PIPE) {
                if !custom.trim().is_empty() {
                    return Self::Windows(custom);
                }
            }
            Self::Windows(Self::DEFAULT_WINDOWS_PIPE.to_string())
        }

        #[cfg(not(windows))]
        {
            if let Ok(custom) = env::var(Self::ENV_UNIX_SOCKET)
                && !custom.trim().is_empty()
            {
                return Self::Unix(PathBuf::from(custom));
            }

            // Honor XDG_RUNTIME_DIR for unprivileged/user-session daemons
            if let Ok(xdg) = env::var("XDG_RUNTIME_DIR")
                && !xdg.trim().is_empty()
            {
                return Self::Unix(Path::new(&xdg).join("taskforest").join("daemon.sock"));
            }

            Self::Unix(PathBuf::from(Self::DEFAULT_UNIX_PATH))
        }
    }

    /// Construct a custom Unix Domain Socket endpoint.
    #[must_use]
    pub fn unix(path: impl Into<PathBuf>) -> Self {
        Self::Unix(path.into())
    }

    /// Construct a custom Windows Named Pipe endpoint.
    #[must_use]
    pub fn windows(pipe_name: impl Into<String>) -> Self {
        Self::Windows(pipe_name.into())
    }
}
