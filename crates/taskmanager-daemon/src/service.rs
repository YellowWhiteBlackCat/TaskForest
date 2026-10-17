//! Main daemon service orchestrator, transport lifecycle, and client broadcast loop.

#[cfg(not(windows))]
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use crossbeam_channel::{Sender, TrySendError, bounded};
use taskmanager_core::unix_millis;
#[cfg(not(windows))]
use taskmanager_ipc::IpcEndpoint;
use taskmanager_ipc::protocol::{ClientMessage, DaemonMessage, Frame, SubscriptionMask};
use taskmanager_ipc::service::ClientSession;
use taskmanager_ipc::transport::{FramedReader, FramedWriter, TransportStream, bind_transport};

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
    subscribers: Arc<Mutex<Vec<Sender<DaemonMessage>>>>,
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
            subscribers: Arc::new(Mutex::new(Vec::new())),
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

    /// Broadcast a daemon message to all connected and registered client subscriber channels.
    pub fn broadcast(&self, msg: &DaemonMessage) {
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.retain(|sender| match sender.try_send(msg.clone()) {
                Ok(_) => true,
                Err(TrySendError::Full(_)) => true, // slow client: skip frame but preserve session
                Err(TrySendError::Disconnected(_)) => false, // client disconnected: prune
            });
        }
    }

    /// Count active subscribers currently receiving live broadcasts.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.lock().map_or(0, |s| s.len())
    }

    /// Handle one accepted incoming client transport stream by spawning concurrent read/write workers.
    pub fn handle_incoming_connection(
        self: &Arc<Self>,
        stream: Box<dyn TransportStream>,
    ) -> std::io::Result<()> {
        let mut read_stream = stream.try_clone()?;
        let mut write_stream = stream;

        let (tx, rx) = bounded::<DaemonMessage>(64);
        if let Ok(mut subs) = self.subscribers.lock() {
            subs.push(tx.clone());
        }

        // 1. Dedicated client writer thread pushing framed responses and live broadcasts
        thread::spawn(move || {
            let mut writer = FramedWriter::new();
            let mut seq = 0u32;
            while let Ok(msg) = rx.recv() {
                seq = seq.wrapping_add(1);
                if let Ok(payload) = msg.encode_payload() {
                    let frame = Frame::new(msg.opcode(), seq, payload);
                    if writer.write_frame(&mut write_stream, &frame).is_err() {
                        break;
                    }
                }
            }
        });

        // 2. Dedicated client reader thread processing requests from frontend
        let service = Arc::clone(self);
        thread::spawn(move || {
            let mut reader = FramedReader::new();
            let mut session: Option<ClientSession> = None;
            let daemon_version = env!("CARGO_PKG_VERSION");

            while service.is_running() {
                let frame = match reader.read_frame(&mut read_stream) {
                    Ok(f) => f,
                    Err(_) => break, // EOF or client disconnected
                };

                let client_msg =
                    match ClientMessage::decode_payload(frame.header.opcode, &frame.payload) {
                        Ok(m) => m,
                        Err(_) => break,
                    };

                let now_ms = unix_millis(SystemTime::now());
                match client_msg {
                    ClientMessage::Hello {
                        client_version,
                        client_name,
                        pid,
                    } => {
                        let session_id = now_ms.wrapping_add(pid as u64);
                        let new_session = ClientSession::new(
                            session_id,
                            client_name,
                            client_version,
                            pid,
                            now_ms,
                        );
                        if let Ok(mut hub) = service.state.hub.lock() {
                            hub.register_session(new_session.clone());
                        }
                        session = Some(new_session);

                        let welcome = DaemonMessage::Welcome {
                            daemon_version: daemon_version.to_string(),
                            supported_facets: SubscriptionMask::ALL,
                            min_interval_ms: 100,
                        };
                        let _ = tx.send(welcome);
                    }
                    ClientMessage::Subscribe {
                        facets,
                        sample_interval_ms,
                    } => {
                        if let Some(s) = &mut session {
                            s.update_subscription(facets, sample_interval_ms, now_ms);
                            if let Ok(mut hub) = service.state.hub.lock() {
                                hub.update_subscription(
                                    s.session_id,
                                    facets,
                                    sample_interval_ms,
                                    now_ms,
                                );
                            }
                        }
                    }
                    ClientMessage::Ping { nonce } => {
                        let pong = DaemonMessage::Pong {
                            nonce,
                            server_time_ms: now_ms,
                        };
                        let _ = tx.send(pong);
                    }
                    ClientMessage::QueryHistory {
                        facet_mask,
                        range_start_ms,
                        range_end_ms,
                    } => {
                        let (points_count, payload_json) = service.history.query_facet_range(
                            facet_mask,
                            range_start_ms,
                            range_end_ms,
                        );
                        let resp = DaemonMessage::HistoryResponse {
                            facet_mask,
                            points_count,
                            payload_json,
                        };
                        let _ = tx.send(resp);
                    }
                }
            }

            // Cleanup session on client disconnect
            if let Some(s) = session {
                if let Ok(mut hub) = service.state.hub.lock() {
                    hub.unregister_session(s.session_id);
                }
            }
            drop(tx);
        });

        Ok(())
    }

    /// Run the daemon service loop on a dedicated thread, returning a handle to trigger shutdown.
    pub fn start_on_thread(self: Arc<Self>) -> std::io::Result<thread::JoinHandle<()>> {
        self.running.store(true, Ordering::Release);
        let service = Arc::clone(&self);

        let listener = bind_transport(&self.config.endpoint)?;

        // Dedicated accept thread listening for incoming client connections
        let service_accept = Arc::clone(&service);
        thread::spawn(move || {
            while service_accept.is_running() {
                match listener.accept() {
                    Ok(stream) => {
                        let _ = service_accept.handle_incoming_connection(stream);
                    }
                    Err(_) => {
                        if !service_accept.is_running() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            }
        });

        // Main sampling, persistence, and telemetry broadcasting loop
        let handle = thread::spawn(move || {
            let mut last_tick_ms = 0u64;
            let mut collector = DaemonCollector::new();

            while service.is_running() {
                let now_ms = unix_millis(SystemTime::now());
                let cadence = service.state.current_cadence();
                let interval = match cadence {
                    DaemonCadenceMode::Active { interval_ms } => interval_ms,
                    DaemonCadenceMode::Idle => service.config.idle_interval_ms,
                };

                if now_ms.saturating_sub(last_tick_ms) >= interval as u64 {
                    if let Ok(snapshot) = collector.sample_and_record(now_ms, &service.history) {
                        let telem = DaemonMessage::TelemetryBatch {
                            timestamp_ms: now_ms,
                            facet_mask: SubscriptionMask::ALL.bits(),
                            payload_json: serde_json::to_string(&snapshot).unwrap_or_default(),
                        };
                        service.broadcast(&telem);
                    }
                    let _ = service.history.flush(now_ms);
                    last_tick_ms = now_ms;
                }

                thread::sleep(Duration::from_millis(50));
            }

            #[cfg(not(windows))]
            if let IpcEndpoint::Unix(path) = &service.config.endpoint {
                let _ = fs::remove_file(path);
            }
            service.history.close();
        });

        Ok(handle)
    }
}
