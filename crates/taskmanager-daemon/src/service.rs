//! Main daemon service orchestrator, socket lifecycle, and client connection loop.

#[cfg(not(windows))]
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

#[cfg(not(windows))]
use taskmanager_ipc::IpcConnectionHandler;
#[cfg(not(windows))]
use taskmanager_ipc::IpcEndpoint;

use crate::collector::DaemonCollector;
use crate::config::DaemonConfig;
use crate::persistence::DaemonHistoryManager;
use crate::state::{DaemonCadenceMode, DaemonState};

/// Daemon service coordinator managing lifecycle, sampling loops, and client IPC connections.
pub struct DaemonService {
    config: DaemonConfig,
    state: Arc<DaemonState>,
    history: Arc<DaemonHistoryManager>,
    running: Arc<AtomicBool>,
}

impl DaemonService {
    /// Construct a new daemon service with the provided configuration and persistence manager.
    pub fn new(config: DaemonConfig, history: DaemonHistoryManager) -> Self {
        let state = Arc::new(DaemonState::new(
            config.active_interval_ms,
            config.idle_interval_ms,
        ));
        Self {
            config,
            state,
            history: Arc::new(history),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Access daemon runtime configuration.
    #[must_use]
    pub fn config(&self) -> &DaemonConfig {
        &self.config
    }

    /// Access daemon runtime state and subscription metrics.
    #[must_use]
    pub fn state(&self) -> &Arc<DaemonState> {
        &self.state
    }

    /// Access underlying background history persistence manager.
    #[must_use]
    pub fn history(&self) -> &Arc<DaemonHistoryManager> {
        &self.history
    }

    /// Stop the running daemon service.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }

    /// Check if the service is currently marked running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Run the daemon service loop on a dedicated thread, returning a handle to trigger shutdown.
    #[cfg(not(windows))]
    pub fn start_on_thread(self: Arc<Self>) -> std::io::Result<thread::JoinHandle<()>> {
        self.running.store(true, Ordering::Release);
        let service = Arc::clone(&self);

        let socket_path = match &self.config.endpoint {
            IpcEndpoint::Unix(path) => path.clone(),
            IpcEndpoint::Windows(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "Named Pipes unsupported on non-Windows",
                ));
            }
        };

        // Prepare parent directories and clean up stale socket file if present
        if let Some(parent) = socket_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::remove_file(&socket_path);

        let listener = std::os::unix::net::UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;

        let handle = thread::spawn(move || {
            let mut last_tick_ms = 0u64;
            let mut collector = DaemonCollector::new();

            while service.is_running() {
                // 1. Accept incoming client connections non-blockingly
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let hub_clone = Arc::clone(&service.state.hub);
                        thread::spawn(move || {
                            let mut handler = IpcConnectionHandler::new(
                                stream,
                                hub_clone,
                                env!("CARGO_PKG_VERSION"),
                            );
                            let now_ms = 1000;
                            while let Ok(true) = handler.process_one_message(now_ms) {
                                // continue processing frames until client drops
                            }
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No new client, continue
                    }
                    Err(_) => break,
                }

                // 2. Perform periodic background cadence ticks and flushes
                let now_ms = 2000u64;
                let cadence = service.state.current_cadence();
                let interval = match cadence {
                    DaemonCadenceMode::Active { interval_ms } => interval_ms,
                    DaemonCadenceMode::Idle => service.config.idle_interval_ms,
                };

                if now_ms.saturating_sub(last_tick_ms) >= interval as u64 {
                    let _ = collector.sample_and_record(now_ms, &service.history);
                    let _ = service.history.flush(now_ms);
                    last_tick_ms = now_ms;
                }

                thread::sleep(Duration::from_millis(50));
            }

            // Cleanup socket on graceful shutdown
            let _ = fs::remove_file(&socket_path);
            service.history.close();
        });

        Ok(handle)
    }

    /// Run the daemon service loop on Windows.
    #[cfg(windows)]
    pub fn start_on_thread(self: Arc<Self>) -> std::io::Result<thread::JoinHandle<()>> {
        self.running.store(true, Ordering::Release);
        let service = Arc::clone(&self);
        let handle = thread::spawn(move || {
            let mut last_tick_ms = 0u64;
            let mut collector = DaemonCollector::new();

            while service.is_running() {
                let now_ms = 2000u64;
                let cadence = service.state.current_cadence();
                let interval = match cadence {
                    DaemonCadenceMode::Active { interval_ms } => interval_ms,
                    DaemonCadenceMode::Idle => service.config.idle_interval_ms,
                };

                if now_ms.saturating_sub(last_tick_ms) >= interval as u64 {
                    let _ = collector.sample_and_record(now_ms, &service.history);
                    let _ = service.history.flush(now_ms);
                    last_tick_ms = now_ms;
                }

                thread::sleep(Duration::from_millis(100));
            }
            service.history.close();
        });
        Ok(handle)
    }
}
