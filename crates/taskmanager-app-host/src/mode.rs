//! Dual-mode execution engine probe under ADR-055: Daemon Mode vs Standalone Fallback.

use std::env;
#[cfg(not(windows))]
use std::time::Duration;

use taskmanager_ipc::protocol::DaemonMessage;
use taskmanager_ipc::service::IpcClient;
use taskmanager_ipc::transport::IpcEndpoint;

/// Environment variable that forces the frontend to run in standalone in-process mode.
pub const ENV_FORCE_STANDALONE: &'static str = "TASKFOREST_FORCE_STANDALONE";

/// Execution topology of the application host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppHostMode {
    /// Standalone in-process direct sampling mode (ADR-036 fallback).
    Standalone,
    /// Connected to the background taskforest-daemon service (ADR-055).
    Daemon {
        daemon_version: String,
        endpoint: IpcEndpoint,
    },
}

impl AppHostMode {
    #[must_use]
    pub fn is_daemon(&self) -> bool {
        matches!(self, Self::Daemon { .. })
    }

    #[must_use]
    pub fn is_standalone(&self) -> bool {
        matches!(self, Self::Standalone)
    }
}

/// Probe whether an active taskforest-daemon service is available for connection.
///
/// Falls back safely and immediately to `AppHostMode::Standalone` if:
/// - `TASKFOREST_FORCE_STANDALONE` is set,
/// - Socket/Pipe does not exist or connection fails,
/// - Daemon handshake times out or errors.
#[must_use]
pub fn probe_host_mode() -> AppHostMode {
    if env::var(ENV_FORCE_STANDALONE).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true")) {
        return AppHostMode::Standalone;
    }

    let endpoint = IpcEndpoint::default_platform();
    probe_endpoint(&endpoint)
}

/// Probe a specific endpoint for a responsive daemon instance.
#[must_use]
pub fn probe_endpoint(endpoint: &IpcEndpoint) -> AppHostMode {
    match endpoint {
        #[cfg(not(windows))]
        IpcEndpoint::Unix(path) => {
            if !path.exists() {
                return AppHostMode::Standalone;
            }

            // Attempt connection with short timeout
            let stream = match std::os::unix::net::UnixStream::connect(path) {
                Ok(s) => s,
                Err(_) => return AppHostMode::Standalone,
            };

            let _ = stream.set_read_timeout(Some(Duration::from_millis(150)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(150)));

            let mut client = IpcClient::new(stream);
            match client.handshake("taskforest-probe", env!("CARGO_PKG_VERSION")) {
                Ok(DaemonMessage::Welcome { daemon_version, .. }) => AppHostMode::Daemon {
                    daemon_version,
                    endpoint: endpoint.clone(),
                },
                _ => AppHostMode::Standalone,
            }
        }
        #[cfg(windows)]
        IpcEndpoint::Windows(pipe_name) => {
            let file = match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(pipe_name)
            {
                Ok(f) => f,
                Err(_) => return AppHostMode::Standalone,
            };
            let mut client = IpcClient::new(file);
            match client.handshake("taskforest-probe", env!("CARGO_PKG_VERSION")) {
                Ok(DaemonMessage::Welcome { daemon_version, .. }) => AppHostMode::Daemon {
                    daemon_version,
                    endpoint: endpoint.clone(),
                },
                _ => AppHostMode::Standalone,
            }
        }
        #[cfg(windows)]
        IpcEndpoint::Unix(_) => AppHostMode::Standalone,
        #[cfg(not(windows))]
        IpcEndpoint::Windows(_) => AppHostMode::Standalone,
    }
}

use crossbeam_channel::{Receiver, bounded};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use taskmanager_ipc::protocol::SubscriptionMask;

/// Background subscription worker pulling live telemetry frames from taskforest-daemon.
pub struct DaemonSubscriptionWorker {
    running: Arc<AtomicBool>,
}

impl DaemonSubscriptionWorker {
    /// Start a dedicated worker thread connected to the daemon endpoint.
    #[must_use]
    pub fn start(
        endpoint: &IpcEndpoint,
        facets: SubscriptionMask,
        interval_ms: u32,
    ) -> Option<(Self, Receiver<DaemonMessage>)> {
        #[cfg(not(windows))]
        {
            let path = match endpoint {
                IpcEndpoint::Unix(p) => p.clone(),
                #[cfg(windows)]
                IpcEndpoint::Windows(_) => return None,
                #[cfg(not(windows))]
                IpcEndpoint::Windows(_) => return None,
            };

            let stream = std::os::unix::net::UnixStream::connect(path).ok()?;
            let mut client = IpcClient::new(stream);
            let _welcome = client
                .handshake("taskforest-frontend", env!("CARGO_PKG_VERSION"))
                .ok()?;
            let _ = client.subscribe(facets, interval_ms);

            let (tx, rx) = bounded(256);
            let running = Arc::new(AtomicBool::new(true));
            let running_clone = Arc::clone(&running);

            thread::spawn(move || {
                while running_clone.load(Ordering::Acquire) {
                    match client.receive_daemon_message() {
                        Ok(msg) => {
                            if tx.send(msg).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            Some((Self { running }, rx))
        }

        #[cfg(windows)]
        {
            let pipe_name = match endpoint {
                IpcEndpoint::Windows(p) => p.clone(),
                IpcEndpoint::Unix(_) => return None,
            };

            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&pipe_name)
                .ok()?;
            let mut client = IpcClient::new(file);
            let _welcome = client
                .handshake("taskforest-frontend", env!("CARGO_PKG_VERSION"))
                .ok()?;
            let _ = client.subscribe(facets, interval_ms);

            let (tx, rx) = bounded(256);
            let running = Arc::new(AtomicBool::new(true));
            let running_clone = Arc::clone(&running);

            thread::spawn(move || {
                while running_clone.load(Ordering::Acquire) {
                    match client.receive_daemon_message() {
                        Ok(msg) => {
                            if tx.send(msg).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });

            Some((Self { running }, rx))
        }
    }

    /// Signal the subscription worker thread to stop.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }
}

impl Drop for DaemonSubscriptionWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
#[path = "../tests/headless/dual_mode_tests.rs"]
mod tests;
