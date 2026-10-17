//! Lightweight IPC client connector for TaskForest frontends.

use std::io::{Read, Write};

use crate::protocol::error::{IpcError, IpcResult};
use crate::protocol::frame::Frame;
use crate::protocol::mask::SubscriptionMask;
use crate::protocol::message::{ClientMessage, DaemonMessage};
use crate::transport::framed::{FramedReader, FramedWriter};

/// IPC client handle connected to the TaskForest background daemon.
pub struct IpcClient<S> {
    stream: S,
    reader: FramedReader,
    writer: FramedWriter,
    sequence: u32,
}

impl<S: Read + Write> IpcClient<S> {
    #[must_use]
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            reader: FramedReader::new(),
            writer: FramedWriter::new(),
            sequence: 0,
        }
    }

    /// Perform initial handshake with the background daemon.
    pub fn handshake(
        &mut self,
        client_name: impl Into<String>,
        client_version: impl Into<String>,
    ) -> IpcResult<DaemonMessage> {
        let msg = ClientMessage::Hello {
            client_name: client_name.into(),
            client_version: client_version.into(),
            pid: std::process::id(),
        };
        self.send_client_message(&msg)?;

        let response = self.receive_daemon_message()?;
        match response {
            DaemonMessage::Welcome { .. } => Ok(response),
            other => Err(IpcError::ProtocolViolation(format!(
                "Expected Welcome from daemon, received: {other:?}"
            ))),
        }
    }

    /// Update on-demand facet subscription bitmask and refresh interval.
    pub fn subscribe(&mut self, facets: SubscriptionMask, interval_ms: u32) -> IpcResult<()> {
        let msg = ClientMessage::Subscribe {
            facets,
            sample_interval_ms: interval_ms,
        };
        self.send_client_message(&msg)
    }

    /// Send a keepalive ping probe.
    pub fn ping(&mut self, nonce: u64) -> IpcResult<()> {
        let msg = ClientMessage::Ping { nonce };
        self.send_client_message(&msg)
    }

    /// Send a raw client message over the wire.
    pub fn send_client_message(&mut self, msg: &ClientMessage) -> IpcResult<()> {
        self.sequence = self.sequence.wrapping_add(1);
        let payload = msg.encode_payload()?;
        let frame = Frame::new(msg.opcode(), self.sequence, payload);
        self.writer.write_frame(&mut self.stream, &frame)
    }

    /// Block and receive the next daemon message.
    pub fn receive_daemon_message(&mut self) -> IpcResult<DaemonMessage> {
        let frame = self.reader.read_frame(&mut self.stream)?;
        DaemonMessage::decode_payload(frame.header.opcode, &frame.payload)
    }
}

#[cfg(not(windows))]
impl IpcClient<std::os::unix::net::UnixStream> {
    /// Connect to a daemon Unix Domain Socket.
    pub fn connect_uds(path: impl AsRef<std::path::Path>) -> IpcResult<Self> {
        let stream = std::os::unix::net::UnixStream::connect(path)?;
        Ok(Self::new(stream))
    }
}
