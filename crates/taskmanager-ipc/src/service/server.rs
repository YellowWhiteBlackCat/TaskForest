//! Lightweight IPC server connection handler for TaskForest Daemon.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use crate::protocol::error::{IpcError, IpcResult};
use crate::protocol::frame::Frame;
use crate::protocol::mask::SubscriptionMask;
use crate::protocol::message::{ClientMessage, DaemonMessage};
use crate::service::session::ClientSession;
use crate::transport::framed::{FramedReader, FramedWriter};

/// Shared state aggregating on-demand subscriptions across all connected frontends.
#[derive(Debug, Default)]
pub struct SubscriptionHub {
    sessions: HashMap<u64, ClientSession>,
    aggregated_mask: SubscriptionMask,
}

impl SubscriptionHub {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_session(&mut self, session: ClientSession) {
        self.sessions.insert(session.session_id, session);
        self.recompute_mask();
    }

    pub fn unregister_session(&mut self, session_id: u64) {
        self.sessions.remove(&session_id);
        self.recompute_mask();
    }

    pub fn update_subscription(
        &mut self,
        session_id: u64,
        mask: SubscriptionMask,
        interval_ms: u32,
        now_ms: u64,
    ) {
        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.update_subscription(mask, interval_ms, now_ms);
            self.recompute_mask();
        }
    }

    #[must_use]
    pub fn aggregated_mask(&self) -> SubscriptionMask {
        self.aggregated_mask
    }

    #[must_use]
    pub fn active_session_count(&self) -> usize {
        self.sessions.len()
    }

    fn recompute_mask(&mut self) {
        let mut union = SubscriptionMask::empty();
        for session in self.sessions.values() {
            union = union | session.subscription;
        }
        self.aggregated_mask = union;
    }
}

/// Provider trait for resolving historical telemetry queries from persistent storage.
pub trait HistoryProvider: Send + Sync + 'static {
    /// Query historical points matching facet mask and millisecond range, returning (count, json).
    fn query_history(&self, facet_mask: u32, start_ms: u64, end_ms: u64) -> (usize, String);
}

/// Server connection processor handling one connected client stream.
pub struct IpcConnectionHandler<S> {
    stream: S,
    reader: FramedReader,
    writer: FramedWriter,
    sequence: u32,
    session: Option<ClientSession>,
    hub: Arc<Mutex<SubscriptionHub>>,
    daemon_version: String,
    history_provider: Option<Arc<dyn HistoryProvider>>,
}

impl<S: Read + Write> IpcConnectionHandler<S> {
    #[must_use]
    pub fn new(
        stream: S,
        hub: Arc<Mutex<SubscriptionHub>>,
        daemon_version: impl Into<String>,
    ) -> Self {
        Self {
            stream,
            reader: FramedReader::new(),
            writer: FramedWriter::new(),
            sequence: 0,
            session: None,
            hub,
            daemon_version: daemon_version.into(),
            history_provider: None,
        }
    }

    /// Attach a persistent history provider to serve historical range queries.
    #[must_use]
    pub fn with_history_provider(mut self, provider: Arc<dyn HistoryProvider>) -> Self {
        self.history_provider = Some(provider);
        self
    }

    /// Read and dispatch one incoming frame from this client.
    ///
    /// Returns `Ok(true)` if connection remains open, `Ok(false)` if peer closed cleanly.
    pub fn process_one_message(&mut self, now_ms: u64) -> IpcResult<bool> {
        let frame = match self.reader.read_frame(&mut self.stream) {
            Ok(f) => f,
            Err(IpcError::Closed) => {
                self.cleanup();
                return Ok(false);
            }
            Err(e) => return Err(e),
        };

        let client_msg = ClientMessage::decode_payload(frame.header.opcode, &frame.payload)?;
        self.handle_client_message(client_msg, now_ms)?;
        Ok(true)
    }

    fn handle_client_message(&mut self, msg: ClientMessage, now_ms: u64) -> IpcResult<()> {
        match msg {
            ClientMessage::Hello {
                client_version,
                client_name,
                pid,
            } => {
                let session_id = now_ms.wrapping_add(pid as u64);
                let session =
                    ClientSession::new(session_id, client_name, client_version, pid, now_ms);
                self.hub
                    .lock()
                    .map_err(|_| IpcError::ProtocolViolation("Hub lock poisoned".to_string()))?
                    .register_session(session.clone());
                self.session = Some(session);

                let welcome = DaemonMessage::Welcome {
                    daemon_version: self.daemon_version.clone(),
                    supported_facets: SubscriptionMask::ALL,
                    min_interval_ms: 100,
                };
                self.send_daemon_message(&welcome)
            }
            ClientMessage::Subscribe {
                facets,
                sample_interval_ms,
            } => {
                if let Some(session) = &mut self.session {
                    session.update_subscription(facets, sample_interval_ms, now_ms);
                    self.hub
                        .lock()
                        .map_err(|_| IpcError::ProtocolViolation("Hub lock poisoned".to_string()))?
                        .update_subscription(
                            session.session_id,
                            facets,
                            sample_interval_ms,
                            now_ms,
                        );
                }
                Ok(())
            }
            ClientMessage::Ping { nonce } => {
                let pong = DaemonMessage::Pong {
                    nonce,
                    server_time_ms: now_ms,
                };
                self.send_daemon_message(&pong)
            }
            ClientMessage::QueryHistory {
                facet_mask,
                range_start_ms,
                range_end_ms,
            } => {
                let (points_count, payload_json) = if let Some(provider) = &self.history_provider {
                    provider.query_history(facet_mask, range_start_ms, range_end_ms)
                } else {
                    (0, "[]".to_string())
                };
                let resp = DaemonMessage::HistoryResponse {
                    facet_mask,
                    points_count,
                    payload_json,
                };
                self.send_daemon_message(&resp)
            }
        }
    }

    pub fn send_daemon_message(&mut self, msg: &DaemonMessage) -> IpcResult<()> {
        self.sequence = self.sequence.wrapping_add(1);
        let payload = msg.encode_payload()?;
        let frame = Frame::new(msg.opcode(), self.sequence, payload);
        self.writer.write_frame(&mut self.stream, &frame)
    }
}

impl<S> IpcConnectionHandler<S> {
    fn cleanup(&mut self) {
        if let Some(session) = self.session.take()
            && let Ok(mut hub) = self.hub.lock()
        {
            hub.unregister_session(session.session_id);
        }
    }
}

impl<S> Drop for IpcConnectionHandler<S> {
    fn drop(&mut self) {
        self.cleanup();
    }
}
