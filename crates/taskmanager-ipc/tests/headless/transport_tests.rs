//! test-intent: behavior
//!
//! Behavior tests for taskmanager-ipc transport framing, limits, and endpoint resolution.

use std::io::Cursor;

use taskmanager_ipc::protocol::{Frame, FrameHeader, IpcError, Opcode};
use taskmanager_ipc::transport::{FramedReader, FramedWriter, IpcEndpoint};

#[cfg(not(windows))]
use taskmanager_ipc::transport::{bind_transport, connect_transport};
#[cfg(not(windows))]
use taskmanager_test_support::unix_socket::socket_temp_root;

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

#[cfg(not(windows))]
#[test]
fn transport_listener_accept_and_stream_clone() {
    // The socket path must fit `sockaddr_un`'s 108-byte `sun_path`: the local
    // gate exports a LONG `TMPDIR`, so the throwaway socket is anchored at
    // the system's short temp root (see `taskmanager_test_support::unix_socket`).
    let temp_dir = tempfile::Builder::new()
        .prefix("s")
        .tempdir_in(socket_temp_root())
        .unwrap();
    let socket_path = temp_dir.path().join("transport_test.sock");
    let endpoint = IpcEndpoint::Unix(socket_path);

    let listener = bind_transport(&endpoint).unwrap();
    let client_stream = connect_transport(&endpoint).unwrap();
    let server_stream = listener.accept().unwrap();

    let cloned_client = client_stream.try_clone().unwrap();
    let cloned_server = server_stream.try_clone().unwrap();
    assert!(cloned_client.try_clone().is_ok());
    assert!(cloned_server.try_clone().is_ok());
}
