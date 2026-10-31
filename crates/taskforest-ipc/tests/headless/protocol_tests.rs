//! test-intent: behavior
//!
//! Behavior tests for taskforest-ipc binary framing, subscription masks, and message encodings.

use taskforest_ipc::protocol::{
    ClientMessage, DaemonMessage, Frame, FrameHeader, HEADER_LEN, IpcError, Opcode,
    SubscriptionMask,
};

#[test]
fn mask_bitwise_operations() {
    let mut mask = SubscriptionMask::empty();
    assert!(mask.is_empty());

    mask.insert(SubscriptionMask::CPU);
    mask.insert(SubscriptionMask::MEMORY);
    assert!(mask.contains(SubscriptionMask::CPU));
    assert!(mask.contains(SubscriptionMask::MEMORY));
    assert!(!mask.contains(SubscriptionMask::DISK));

    mask.remove(SubscriptionMask::CPU);
    assert!(!mask.contains(SubscriptionMask::CPU));
    assert!(mask.contains(SubscriptionMask::MEMORY));

    let hardware = SubscriptionMask::HARDWARE_ALL;
    assert!(hardware.contains(SubscriptionMask::GPU));
    assert!(!hardware.contains(SubscriptionMask::PROCESSES));
}

#[test]
fn client_message_roundtrip() {
    let msg = ClientMessage::Hello {
        client_version: "0.3.0".to_string(),
        client_name: "taskforest-g".to_string(),
        pid: 1234,
    };
    let opcode = msg.opcode();
    let payload = msg.encode_payload().unwrap();
    let decoded = ClientMessage::decode_payload(opcode, &payload).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn daemon_message_roundtrip() {
    let msg = DaemonMessage::Welcome {
        daemon_version: "0.3.0".to_string(),
        supported_facets: SubscriptionMask::ALL,
        min_interval_ms: 100,
    };
    let opcode = msg.opcode();
    let payload = msg.encode_payload().unwrap();
    let decoded = DaemonMessage::decode_payload(opcode, &payload).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn frame_encode_decode_roundtrip() {
    let payload = b"{\"hello\":\"world\"}".to_vec();
    let frame = Frame::new(Opcode::ClientHello, 42, payload.clone());

    let wire = frame.encode();
    assert_eq!(wire.len(), HEADER_LEN + payload.len());

    let mut header_bytes = [0u8; HEADER_LEN];
    header_bytes.copy_from_slice(&wire[..HEADER_LEN]);
    let header = FrameHeader::decode(&header_bytes).unwrap();
    assert_eq!(header, frame.header);

    let decoded = Frame::from_parts(header, wire[HEADER_LEN..].to_vec()).unwrap();
    assert_eq!(decoded.payload, payload);
}

#[test]
fn corrupt_crc_is_rejected() {
    let payload = b"valid data".to_vec();
    let frame = Frame::new(Opcode::Ping, 1, payload);

    let mut header = frame.header;
    header.payload_crc32 ^= 0xFF; // Invert CRC
    let result = Frame::from_parts(header, b"valid data".to_vec());
    assert!(matches!(result, Err(IpcError::CrcMismatch { .. })));
}

#[test]
fn invalid_magic_is_rejected() {
    let mut header_bytes = [0u8; HEADER_LEN];
    header_bytes[..4].copy_from_slice(b"BAD!");
    let result = FrameHeader::decode(&header_bytes);
    assert!(matches!(result, Err(IpcError::MagicMismatch(_))));
}
