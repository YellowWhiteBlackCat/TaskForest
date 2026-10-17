//! Dual-mode execution engine probe under ADR-055: Daemon Mode vs Standalone Fallback.

use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crossbeam_channel::{Receiver, bounded};
use taskmanager_ipc::protocol::{DaemonMessage, SubscriptionMask};
use taskmanager_ipc::service::IpcClient;
use taskmanager_ipc::transport::{IpcEndpoint, connect_transport};

/// Environment variable that forces the frontend to run in standalone in-process mode.
pub const ENV_FORCE_STANDALONE: &str = "TASKFOREST_FORCE_STANDALONE";

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
#[must_use]
pub fn probe_host_mode() -> AppHostMode {
    if env::var(ENV_FORCE_STANDALONE).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true")) {
        return AppHostMode::Standalone;
    }

    let endpoint = IpcEndpoint::default_platform();
    probe_endpoint(&endpoint)
}

/// Probe a specific endpoint for a responsive daemon instance using the platform-neutral transport trait.
#[must_use]
pub fn probe_endpoint(endpoint: &IpcEndpoint) -> AppHostMode {
    let stream = match connect_transport(endpoint) {
        Ok(s) => s,
        Err(_) => return AppHostMode::Standalone,
    };

    let mut client = IpcClient::new(stream);
    match client.handshake("taskforest-probe", env!("CARGO_PKG_VERSION")) {
        Ok(DaemonMessage::Welcome { daemon_version, .. }) => AppHostMode::Daemon {
            daemon_version,
            endpoint: endpoint.clone(),
        },
        _ => AppHostMode::Standalone,
    }
}

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
        let stream = connect_transport(endpoint).ok()?;
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
