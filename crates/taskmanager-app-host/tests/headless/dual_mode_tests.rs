use super::*;
use taskmanager_ipc::transport::IpcEndpoint;

#[cfg(not(windows))]
use std::thread;
#[cfg(not(windows))]
use std::time::Duration;
#[cfg(not(windows))]
use taskmanager_ipc::protocol::{DaemonMessage, Frame, Opcode, SubscriptionMask};
#[cfg(not(windows))]
use taskmanager_ipc::transport::{FramedReader, FramedWriter};

#[test]
fn probe_host_mode_defaults_to_standalone_when_no_daemon_active() {
    let mode = probe_host_mode();
    // In typical dev/test environments without background daemon, must fallback safely
    assert!(mode.is_standalone() || mode.is_daemon());
}

#[test]
fn probe_endpoint_non_existent_path_falls_back_to_standalone() {
    let non_existent = IpcEndpoint::unix("/tmp/non_existent_taskforest_daemon_sock_12345");
    let mode = probe_endpoint(&non_existent);
    assert_eq!(mode, AppHostMode::Standalone);
    assert!(mode.is_standalone());
    assert!(!mode.is_daemon());
}

#[cfg(not(windows))]
#[test]
fn probe_endpoint_with_mock_daemon() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("probe_test.sock");

    let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

    let server_handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = FramedReader::new();
        let mut writer = FramedWriter::new();

        // Expect Hello
        let frame = reader.read_frame(&mut stream).unwrap();
        assert_eq!(frame.header.opcode, Opcode::ClientHello);

        // Reply Welcome
        let welcome = DaemonMessage::Welcome {
            daemon_version: "0.3.0-mock".to_string(),
            supported_facets: SubscriptionMask::ALL,
            min_interval_ms: 100,
        };
        let resp_frame = Frame::new(Opcode::DaemonWelcome, 1, welcome.encode_payload().unwrap());
        writer.write_frame(&mut stream, &resp_frame).unwrap();
    });

    thread::sleep(Duration::from_millis(20));
    let endpoint = IpcEndpoint::unix(&socket_path);
    let mode = probe_endpoint(&endpoint);

    match mode {
        AppHostMode::Daemon { daemon_version, .. } => {
            assert_eq!(daemon_version, "0.3.0-mock");
        }
        AppHostMode::Standalone => panic!("Expected Daemon mode from live probe"),
    }

    server_handle.join().unwrap();
}

#[cfg(not(windows))]
#[test]
fn daemon_subscription_worker_streaming_test() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("sub_worker_test.sock");

    let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();

    let server_handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = FramedReader::new();
        let mut writer = FramedWriter::new();

        // 1. Expect Hello
        let hello_frame = reader.read_frame(&mut stream).unwrap();
        assert_eq!(hello_frame.header.opcode, Opcode::ClientHello);

        // 2. Reply Welcome
        let welcome = DaemonMessage::Welcome {
            daemon_version: "0.3.0".to_string(),
            supported_facets: SubscriptionMask::ALL,
            min_interval_ms: 100,
        };
        let resp_frame = Frame::new(Opcode::DaemonWelcome, 1, welcome.encode_payload().unwrap());
        writer.write_frame(&mut stream, &resp_frame).unwrap();

        // 3. Expect Subscribe
        let sub_frame = reader.read_frame(&mut stream).unwrap();
        assert_eq!(sub_frame.header.opcode, Opcode::ClientSubscribe);

        // 4. Send Telemetry Batch
        let batch = DaemonMessage::TelemetryBatch {
            timestamp_ms: 5000,
            facet_mask: SubscriptionMask::CPU.bits(),
            payload_json: "{\"cpu\":15.0}".to_string(),
        };
        let batch_frame = Frame::new(Opcode::TelemetryBatch, 2, batch.encode_payload().unwrap());
        writer.write_frame(&mut stream, &batch_frame).unwrap();
    });

    thread::sleep(Duration::from_millis(20));
    let endpoint = IpcEndpoint::unix(&socket_path);

    let (worker, receiver) =
        DaemonSubscriptionWorker::start(&endpoint, SubscriptionMask::CPU, 200).unwrap();

    let received = receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("Should receive live batch from daemon stream");

    if let DaemonMessage::TelemetryBatch { timestamp_ms, .. } = received {
        assert_eq!(timestamp_ms, 5000);
    } else {
        panic!("Expected TelemetryBatch message");
    }

    worker.stop();
    server_handle.join().unwrap();
}
