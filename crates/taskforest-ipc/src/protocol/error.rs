//! Strongly-typed IPC protocol and transport errors.

use std::io;

/// Results returned by taskforest-ipc operations.
pub type IpcResult<T> = Result<T, IpcError>;

/// Protocol and transport errors encountered during IPC communication.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid frame magic: expected b\"TFIP\", found {0:?}")]
    MagicMismatch([u8; 4]),

    #[error("Unsupported protocol version: {0} (current supported version is 1)")]
    UnsupportedVersion(u8),

    #[error("Unknown opcode: 0x{0:02x}")]
    UnknownOpcode(u8),

    #[error("Frame payload length {len} exceeds safety limit of {limit} bytes")]
    PayloadTooLarge { len: usize, limit: usize },

    #[error("CRC32 mismatch: expected 0x{expected:08x}, calculated 0x{calculated:08x}")]
    CrcMismatch { expected: u32, calculated: u32 },

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Protocol contract violation: {0}")]
    ProtocolViolation(String),

    #[error("Connection closed by peer")]
    Closed,
}

impl From<serde_json::Error> for IpcError {
    fn from(error: serde_json::Error) -> Self {
        Self::Deserialization(error.to_string())
    }
}
