//! Low-level frame binary encoding, decoding, and CRC32 payload verification.
//!
//! # Wire Layout (20 bytes fixed header):
//!
//! ```text
//! 0                   1                   2                   3
//! 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |       'T'     |       'F'     |       'I'     |       'P'     |  Magic (4 bytes)
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |    Version    |     Opcode    |             Flags             |  Ver(1), Op(1), Flags(2)
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                       Sequence Number                         |  Sequence (4 bytes, BE)
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                        Payload Length                         |  Length (4 bytes, BE)
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                        Payload CRC-32                         |  CRC-32 (4 bytes, BE)
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                        Payload Data ...                       |  Length bytes
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use crate::protocol::error::{IpcError, IpcResult};
use crate::protocol::message::Opcode;

/// Magic bytes identifying TaskForest IPC frames ("TFIP").
pub const FRAME_MAGIC: [u8; 4] = *b"TFIP";

/// Protocol major version 1.
pub const PROTOCOL_VERSION: u8 = 1;

/// Fixed length of a frame header in bytes.
pub const HEADER_LEN: usize = 20;

/// Default maximum allowed payload size (16 MiB) to guard against runaway allocations or DoS.
pub const MAX_PAYLOAD_LEN: usize = 16 * 1024 * 1024;

/// Fixed-size 20-byte frame header metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub version: u8,
    pub opcode: Opcode,
    pub flags: u16,
    pub sequence: u32,
    pub payload_len: u32,
    pub payload_crc32: u32,
}

impl FrameHeader {
    /// Serialize the header into its 20-byte wire representation.
    #[must_use]
    pub fn encode(&self) -> [u8; HEADER_LEN] {
        let mut buf = [0u8; HEADER_LEN];
        buf[0..4].copy_from_slice(&FRAME_MAGIC);
        buf[4] = self.version;
        buf[5] = self.opcode as u8;
        buf[6..8].copy_from_slice(&self.flags.to_be_bytes());
        buf[8..12].copy_from_slice(&self.sequence.to_be_bytes());
        buf[12..16].copy_from_slice(&self.payload_len.to_be_bytes());
        buf[16..20].copy_from_slice(&self.payload_crc32.to_be_bytes());
        buf
    }

    /// Parse and validate a 20-byte wire buffer into a structured frame header.
    pub fn decode(buf: &[u8; HEADER_LEN]) -> IpcResult<Self> {
        let magic: [u8; 4] = [buf[0], buf[1], buf[2], buf[3]];
        if magic != FRAME_MAGIC {
            return Err(IpcError::MagicMismatch(magic));
        }

        let version = buf[4];
        if version != PROTOCOL_VERSION {
            return Err(IpcError::UnsupportedVersion(version));
        }

        let opcode = Opcode::try_from_u8(buf[5])?;
        let flags = u16::from_be_bytes([buf[6], buf[7]]);
        let sequence = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let payload_len = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]);
        let payload_crc32 = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);

        if (payload_len as usize) > MAX_PAYLOAD_LEN {
            return Err(IpcError::PayloadTooLarge {
                len: payload_len as usize,
                limit: MAX_PAYLOAD_LEN,
            });
        }

        Ok(Self {
            version,
            opcode,
            flags,
            sequence,
            payload_len,
            payload_crc32,
        })
    }
}

/// Structured binary frame bundling header and payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub header: FrameHeader,
    pub payload: Vec<u8>,
}

impl Frame {
    /// Construct a new frame, automatically calculating payload length and CRC32.
    #[must_use]
    pub fn new(opcode: Opcode, sequence: u32, payload: Vec<u8>) -> Self {
        let payload_crc32 = crc32fast::hash(&payload);
        let header = FrameHeader {
            version: PROTOCOL_VERSION,
            opcode,
            flags: 0,
            sequence,
            payload_len: payload.len() as u32,
            payload_crc32,
        };
        Self { header, payload }
    }

    /// Encode the entire frame (header + payload) into a single contiguous byte buffer.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.payload.len());
        out.extend_from_slice(&self.header.encode());
        out.extend_from_slice(&self.payload);
        out
    }

    /// Verify payload against header CRC32 and assemble structured frame.
    pub fn from_parts(header: FrameHeader, payload: Vec<u8>) -> IpcResult<Self> {
        if payload.len() != header.payload_len as usize {
            return Err(IpcError::ProtocolViolation(format!(
                "Payload size mismatch: expected {} bytes, received {} bytes",
                header.payload_len,
                payload.len()
            )));
        }

        let calculated = crc32fast::hash(&payload);
        if calculated != header.payload_crc32 {
            return Err(IpcError::CrcMismatch {
                expected: header.payload_crc32,
                calculated,
            });
        }

        Ok(Self { header, payload })
    }
}
