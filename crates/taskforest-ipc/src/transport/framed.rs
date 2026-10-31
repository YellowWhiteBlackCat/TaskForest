//! Bounded framed reader and writer operating over any standard `Read`/`Write` stream.

use std::io::{self, ErrorKind, Read, Write};

use crate::protocol::error::{IpcError, IpcResult};
use crate::protocol::frame::{Frame, FrameHeader, HEADER_LEN};

/// Synchronous reader that yields structured `Frame` instances with bounds enforcement.
#[derive(Debug, Default)]
pub struct FramedReader {
    header_buf: [u8; HEADER_LEN],
}

impl FramedReader {
    #[must_use]
    pub fn new() -> Self {
        Self {
            header_buf: [0u8; HEADER_LEN],
        }
    }

    /// Read the next frame from a stream.
    ///
    /// Returns `Err(IpcError::Closed)` if the connection closed cleanly at a frame boundary.
    pub fn read_frame<R: Read>(&mut self, reader: &mut R) -> IpcResult<Frame> {
        let mut bytes_read = 0;
        while bytes_read < HEADER_LEN {
            match reader.read(&mut self.header_buf[bytes_read..]) {
                Ok(0) => {
                    if bytes_read == 0 {
                        return Err(IpcError::Closed);
                    }
                    return Err(IpcError::Io(io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "Connection closed mid-header",
                    )));
                }
                Ok(n) => bytes_read += n,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(IpcError::Io(e)),
            }
        }

        let header = FrameHeader::decode(&self.header_buf)?;
        let mut payload = vec![0u8; header.payload_len as usize];

        if !payload.is_empty() {
            reader.read_exact(&mut payload).map_err(|e| {
                if e.kind() == ErrorKind::UnexpectedEof {
                    IpcError::Io(io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "Connection closed mid-payload",
                    ))
                } else {
                    IpcError::Io(e)
                }
            })?;
        }

        Frame::from_parts(header, payload)
    }
}

/// Synchronous writer that serializes `Frame` instances and flushes to the stream.
#[derive(Debug, Default)]
pub struct FramedWriter;

impl FramedWriter {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Encode and write a frame to the underlying sink, flushing immediately.
    pub fn write_frame<W: Write>(&mut self, writer: &mut W, frame: &Frame) -> IpcResult<()> {
        let wire = frame.encode();
        writer.write_all(&wire)?;
        writer.flush()?;
        Ok(())
    }
}
