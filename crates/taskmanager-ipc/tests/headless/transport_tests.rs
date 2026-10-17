//! test-intent: behavior
//!
//! Behavior tests for taskmanager-ipc transport framing, limits, and endpoint resolution.

use std::io::Cursor;

use taskmanager_ipc::protocol::{Frame, FrameHeader, IpcError, Opcode};
use taskmanager_ipc::transport::{FramedReader, FramedWriter, IpcEndpoint};

#[test]
fn framed_read_write_roundtrip() {
    let frame1 = Frame::new(Opcode::Ping, 1, b"ping1".to_vec());
    let frame2 = Frame::new(Opcode::Pong, 2, b"pong2".to_vec());

    let mut stream = Cursor::new(Vec::new());
    let mut writer = FramedWriter::new();
    writer.write_frame(&mut stream, &frame1).unwrap();
    writer.write_frame(&mut stream, &frame2).unwrap();

    stream.set_position(0);
    let mut reader = FramedReader::new();
    let read1 = reader.read_frame(&mut stream).unwrap();
    assert_eq!(read1, frame1);

    let read2 = reader.read_frame(&mut stream).unwrap();
    assert_eq!(read2, frame2);

    // EOF at boundary yields Closed
    let eof = reader.read_frame(&mut stream);
    assert!(matches!(eof, Err(IpcError::Closed)));
}

#[test]
fn corrupted_and_oversized_payload_protection() {
    let mut reader = FramedReader::new();

    // 1. Corrupted CRC
    let mut frame = Frame::new(Opcode::Ping, 1, b"ping".to_vec());
    frame.header.payload_crc32 ^= 0x12345678;
    let mut wire = frame.encode();
    let res = reader.read_frame(&mut &wire[..]);
    assert!(matches!(res, Err(IpcError::CrcMismatch { .. })));

    // 2. Corrupted Magic
    wire[0..4].copy_from_slice(b"NOPE");
    let res = reader.read_frame(&mut &wire[..]);
    assert!(matches!(res, Err(IpcError::MagicMismatch(_))));

    // 3. Oversized payload header rejection
    let mut header = [0u8; 20];
    header[0..4].copy_from_slice(b"TFIP");
    header[4] = 1; // version
    header[5] = Opcode::Ping as u8;
    header[12..16].copy_from_slice(&((32 * 1024 * 1024u32).to_be_bytes())); // 32 MiB > 16 MiB limit
    let res = FrameHeader::decode(&header);
    assert!(matches!(res, Err(IpcError::PayloadTooLarge { .. })));
}

#[test]
fn endpoint_default_platform_resolution() {
    let endpoint = IpcEndpoint::default_platform();
    #[cfg(windows)]
    {
        assert!(matches!(endpoint, IpcEndpoint::Windows(_)));
    }
    #[cfg(not(windows))]
    {
        assert!(matches!(endpoint, IpcEndpoint::Unix(_)));
    }
}
