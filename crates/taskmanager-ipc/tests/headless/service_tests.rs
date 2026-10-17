//! test-intent: integration
//!
//! Integration behavior tests for taskmanager-ipc client and daemon server interaction.

#[cfg(not(windows))]
use std::sync::{Arc, Mutex};
#[cfg(not(windows))]
use std::thread;
#[cfg(not(windows))]
use std::time::Duration;

#[cfg(not(windows))]
use taskmanager_ipc::protocol::{DaemonMessage, Frame, Opcode, SubscriptionMask};
#[cfg(not(windows))]
use taskmanager_ipc::service::{IpcClient, IpcConnectionHandler, SubscriptionHub};
#[cfg(not(windows))]
use taskmanager_ipc::transport::{FramedReader, FramedWriter};

#[cfg(not(windows))]
#[test]
fn e2e_uds_handshake_subscribe_and_telem_stream() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("test_daemon.sock");

    let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    let hub = Arc::new(Mutex::new(SubscriptionHub::new()));
    let hub_clone = Arc::clone(&hub);

    // Spawn daemon server handler thread
    let server_handle = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut handler = IpcConnectionHandler::new(stream, hub_clone, "0.3.0");

        // 1. Process Client Hello -> Reply Welcome
        assert!(handler.process_one_message(1000).unwrap());

        // 2. Process Client Subscribe
        assert!(handler.process_one_message(1010).unwrap());

        // 3. Process Client Ping -> Reply Pong
        assert!(handler.process_one_message(1020).unwrap());

        // 4. Push Telemetry Batch to client
        let telem = DaemonMessage::TelemetryBatch {
            timestamp_ms: 1050,
            facet_mask: SubscriptionMask::CPU.bits(),
            payload_json: "{\"cpu_percent\":12.5}".to_string(),
        };
        handler.send_daemon_message(&telem).unwrap();

        // 5. Wait for client disconnect
        assert!(!handler.process_one_message(1060).unwrap());
    });

    // Client connection thread
    let mut client = IpcClient::connect_uds(&socket_path).unwrap();

    // 1. Handshake
    let welcome = client.handshake("taskforest-g", "0.3.0").unwrap();
    if let DaemonMessage::Welcome { daemon_version, .. } = welcome {
        assert_eq!(daemon_version, "0.3.0");
    } else {
        panic!("Expected Welcome message");
    }

    // Verify server hub sees active session
    thread::sleep(Duration::from_millis(10));
    assert_eq!(hub.lock().unwrap().active_session_count(), 1);

    // 2. Subscribe to CPU and MEMORY
    let sub_mask = SubscriptionMask::CPU | SubscriptionMask::MEMORY;
    client.subscribe(sub_mask, 250).unwrap();

    thread::sleep(Duration::from_millis(10));
    assert_eq!(hub.lock().unwrap().aggregated_mask(), sub_mask);

    // 3. Ping / Pong
    client.ping(0xDEADBEEF).unwrap();
    let pong = client.receive_daemon_message().unwrap();
    assert_eq!(
        pong,
        DaemonMessage::Pong {
            nonce: 0xDEADBEEF,
            server_time_ms: 1020
        }
    );

    // 4. Receive telemetry batch
    let batch = client.receive_daemon_message().unwrap();
    if let DaemonMessage::TelemetryBatch {
        timestamp_ms,
        facet_mask,
        payload_json,
    } = batch
    {
        assert_eq!(timestamp_ms, 1050);
        assert_eq!(facet_mask, SubscriptionMask::CPU.bits());
        assert_eq!(payload_json, "{\"cpu_percent\":12.5}");
    } else {
        panic!("Expected TelemetryBatch message");
    }

    // 5. Drop client connection
    drop(client);
    server_handle.join().unwrap();

    // Verify session cleanup
    assert_eq!(hub.lock().unwrap().active_session_count(), 0);
    assert_eq!(
        hub.lock().unwrap().aggregated_mask(),
        SubscriptionMask::empty()
    );
}

#[cfg(not(windows))]
#[test]
fn high_frequency_batch_throughput_and_checksum_integrity() {
    let (client_stream, server_stream) = mock_duplex();

    let server_thread = thread::spawn(move || {
        let mut writer = FramedWriter::new();
        let mut stream = server_stream;

        for seq in 0..1000 {
            let payload =
                format!("{{\"sample_id\":{seq},\"value\":{:.2}}}", seq as f32 * 0.1).into_bytes();
            let frame = Frame::new(Opcode::TelemetryBatch, seq, payload);
            writer.write_frame(&mut stream, &frame).unwrap();
        }
    });

    let mut reader = FramedReader::new();
    let mut stream = client_stream;

    for seq in 0..1000 {
        let frame = reader.read_frame(&mut stream).unwrap();
        assert_eq!(frame.header.sequence, seq);
        assert_eq!(frame.header.opcode, Opcode::TelemetryBatch);
        let expected_payload =
            format!("{{\"sample_id\":{seq},\"value\":{:.2}}}", seq as f32 * 0.1).into_bytes();
        assert_eq!(frame.payload, expected_payload);
    }

    server_thread.join().unwrap();
}

#[cfg(not(windows))]
fn mock_duplex() -> (MockStream, MockStream) {
    let (s1, s2) = std::os::unix::net::UnixStream::pair().unwrap();
    (MockStream(s1), MockStream(s2))
}

#[cfg(not(windows))]
struct MockStream(std::os::unix::net::UnixStream);

#[cfg(not(windows))]
impl std::io::Read for MockStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

#[cfg(not(windows))]
impl std::io::Write for MockStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}
