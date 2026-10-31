//! Strongly-typed client and daemon messages across the IPC wire.

use crate::protocol::error::{IpcError, IpcResult};
use crate::protocol::mask::SubscriptionMask;
use serde::{Deserialize, Serialize};

/// Fixed 1-byte opcode designating the message payload variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Opcode {
    ClientHello = 0x01,
    DaemonWelcome = 0x02,
    ClientSubscribe = 0x03,
    TelemetryBatch = 0x04,
    QueryHistory = 0x05,
    HistoryResponse = 0x06,
    Ping = 0x07,
    Pong = 0x08,
    DaemonError = 0x09,
}

impl Opcode {
    pub fn try_from_u8(value: u8) -> IpcResult<Self> {
        match value {
            0x01 => Ok(Self::ClientHello),
            0x02 => Ok(Self::DaemonWelcome),
            0x03 => Ok(Self::ClientSubscribe),
            0x04 => Ok(Self::TelemetryBatch),
            0x05 => Ok(Self::QueryHistory),
            0x06 => Ok(Self::HistoryResponse),
            0x07 => Ok(Self::Ping),
            0x08 => Ok(Self::Pong),
            0x09 => Ok(Self::DaemonError),
            other => Err(IpcError::UnknownOpcode(other)),
        }
    }
}

/// Messages emitted from client frontend instances to the background daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Initial client handshake announcing presence and version.
    Hello {
        client_version: String,
        client_name: String,
        pid: u32,
    },
    /// Update on-demand subscription facets and desired refresh cadence.
    Subscribe {
        facets: SubscriptionMask,
        sample_interval_ms: u32,
    },
    /// Request persistent historical timeline records for a specific facet.
    QueryHistory {
        facet_mask: u32,
        range_start_ms: u64,
        range_end_ms: u64,
    },
    /// Keepalive heartbeat probe with client timestamp nonce.
    Ping { nonce: u64 },
}

impl ClientMessage {
    #[must_use]
    pub fn opcode(&self) -> Opcode {
        match self {
            Self::Hello { .. } => Opcode::ClientHello,
            Self::Subscribe { .. } => Opcode::ClientSubscribe,
            Self::QueryHistory { .. } => Opcode::QueryHistory,
            Self::Ping { .. } => Opcode::Ping,
        }
    }

    pub fn encode_payload(&self) -> IpcResult<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| IpcError::Serialization(e.to_string()))
    }

    pub fn decode_payload(opcode: Opcode, payload: &[u8]) -> IpcResult<Self> {
        match opcode {
            Opcode::ClientHello | Opcode::ClientSubscribe | Opcode::QueryHistory | Opcode::Ping => {
                serde_json::from_slice(payload)
                    .map_err(|e| IpcError::Deserialization(e.to_string()))
            }
            other => Err(IpcError::ProtocolViolation(format!(
                "Unexpected daemon opcode {other:?} received as client message"
            ))),
        }
    }
}

/// Messages emitted from the background daemon to connected client frontends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DaemonMessage {
    /// Acknowledgment of client connection with daemon capabilities.
    Welcome {
        daemon_version: String,
        supported_facets: SubscriptionMask,
        min_interval_ms: u32,
    },
    /// Real-time live telemetry observation batch.
    TelemetryBatch {
        timestamp_ms: u64,
        facet_mask: u32,
        payload_json: String,
    },
    /// Historical telemetry response stream.
    HistoryResponse {
        facet_mask: u32,
        points_count: usize,
        payload_json: String,
    },
    /// Heartbeat response echoing client probe.
    Pong { nonce: u64, server_time_ms: u64 },
    /// Structured daemon-side processing or permission error.
    Error { code: u16, message: String },
}

impl DaemonMessage {
    #[must_use]
    pub fn opcode(&self) -> Opcode {
        match self {
            Self::Welcome { .. } => Opcode::DaemonWelcome,
            Self::TelemetryBatch { .. } => Opcode::TelemetryBatch,
            Self::HistoryResponse { .. } => Opcode::HistoryResponse,
            Self::Pong { .. } => Opcode::Pong,
            Self::Error { .. } => Opcode::DaemonError,
        }
    }

    pub fn encode_payload(&self) -> IpcResult<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| IpcError::Serialization(e.to_string()))
    }

    pub fn decode_payload(opcode: Opcode, payload: &[u8]) -> IpcResult<Self> {
        match opcode {
            Opcode::DaemonWelcome
            | Opcode::TelemetryBatch
            | Opcode::HistoryResponse
            | Opcode::Pong
            | Opcode::DaemonError => serde_json::from_slice(payload)
                .map_err(|e| IpcError::Deserialization(e.to_string())),
            other => Err(IpcError::ProtocolViolation(format!(
                "Unexpected client opcode {other:?} received as daemon message"
            ))),
        }
    }
}
