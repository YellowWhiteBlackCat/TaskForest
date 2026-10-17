//! test-intent: integration
//!
//! Behavior tests for daemon service lifecycle, state machine, and persistence integration.

#[cfg(not(windows))]
use std::sync::Arc;
#[cfg(not(windows))]
use std::thread;
#[cfg(not(windows))]
use std::time::Duration;
#[cfg(not(windows))]
use taskmanager_core::unix_millis;
use taskmanager_core::{HistoricalSample, HistoryMetric, HistorySeriesKey};
use taskmanager_daemon::collector::DaemonCollector;
#[cfg(not(windows))]
use taskmanager_daemon::config::DaemonConfig;
use taskmanager_daemon::persistence::DaemonHistoryManager;
#[cfg(not(windows))]
use taskmanager_daemon::service::DaemonService;
use taskmanager_daemon::state::{DaemonCadenceMode, DaemonState};
use taskmanager_ipc::protocol::SubscriptionMask;
#[cfg(not(windows))]
use taskmanager_ipc::protocol::{ClientMessage, DaemonMessage};
use taskmanager_ipc::service::ClientSession;
#[cfg(not(windows))]
use taskmanager_ipc::service::IpcClient;
#[cfg(not(windows))]
use taskmanager_ipc::transport::IpcEndpoint;

#[test]
fn daemon_configuration_and_cadence_switching() {
    let state = DaemonState::new(200, 5000);
    assert_eq!(state.current_cadence(), DaemonCadenceMode::Idle);
    assert_eq!(state.client_count(), 0);

    // Register an active client session with CPU subscription
    let mut session =
        ClientSession::new(1, "test-client".to_string(), "0.3.0".to_string(), 100, 1000);
    session.update_subscription(SubscriptionMask::CPU, 200, 1000);

    state.hub.lock().unwrap().register_session(session);
    assert_eq!(state.client_count(), 1);
    assert_eq!(
        state.current_cadence(),
        DaemonCadenceMode::Active { interval_ms: 200 }
    );
    assert_eq!(state.active_mask(), SubscriptionMask::CPU);

    // Unregister session -> reverts to Idle
    state.hub.lock().unwrap().unregister_session(1);
    assert_eq!(state.client_count(), 0);
    assert_eq!(state.current_cadence(), DaemonCadenceMode::Idle);
}

#[test]
fn daemon_history_persistence_and_flush() {
    let temp_dir = tempfile::tempdir().unwrap();
    let history_dir = temp_dir.path().join("daemon_history");

    let history = DaemonHistoryManager::open(&history_dir).unwrap();
    let key = HistorySeriesKey::system(HistoryMetric::CpuUsagePct);
    let sample = HistoricalSample {
        revision: 1,
        completed_at_ms: 1000,
        measured_at_ms: Some(1000),
        value: Some(45.5),
    };
    let res = history.record_sample(key, sample);
    assert!(res.is_ok());

    let flush_res = history.flush(1050);
    assert!(flush_res.is_ok());

    // Second writer is rejected under ADR-055 single-writer lock
    let second_writer = DaemonHistoryManager::open(&history_dir);
    assert!(second_writer.is_err());

    history.close();
}

#[cfg(not(windows))]
#[test]
fn daemon_service_socket_lifecycle_and_graceful_shutdown() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("daemon_service.sock");
    let history_dir = temp_dir.path().join("daemon_svc_history");

    let config = DaemonConfig::new(IpcEndpoint::unix(&socket_path), &history_dir);
    let history = DaemonHistoryManager::open(&history_dir).unwrap();
    let service = Arc::new(DaemonService::new(config, history));

    let join_handle = Arc::clone(&service).start_on_thread().unwrap();
    thread::sleep(Duration::from_millis(50));
    assert!(socket_path.exists());

    // Connect client and handshake
    let mut client = IpcClient::connect_uds(&socket_path).unwrap();
    let welcome = client.handshake("test-gui", "0.3.0").unwrap();
    assert!(matches!(welcome, DaemonMessage::Welcome { .. }));

    // Stop service and verify clean shutdown
    service.stop();
    join_handle.join().unwrap();
    assert!(!service.is_running());
    assert!(!socket_path.exists());
}

#[test]
fn daemon_collector_real_sampling_test() {
    let temp_dir = tempfile::tempdir().unwrap();
    let history_dir = temp_dir.path().join("collector_history");
    let history = DaemonHistoryManager::open(&history_dir).unwrap();

    let mut collector = DaemonCollector::new();
    let snapshot = collector.sample_and_record(1000, &history).unwrap();
    assert_eq!(snapshot.timestamp_ms, 1000);

    #[cfg(target_os = "linux")]
    {
        // On Linux host, memory total must be positive non-zero
        assert!(snapshot.memory_total_bytes.is_some_and(|tot| tot > 0));
    }
}

#[cfg(not(windows))]
#[test]
fn e2e_daemon_telemetry_broadcast_to_subscribers() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("broadcast_daemon.sock");
    let history_dir = temp_dir.path().join("broadcast_history");

    let mut config = DaemonConfig::new(IpcEndpoint::unix(&socket_path), &history_dir);
    config.active_interval_ms = 100;
    let history = DaemonHistoryManager::open(&history_dir).unwrap();
    let service = Arc::new(DaemonService::new(config, history));

    let join_handle = Arc::clone(&service).start_on_thread().unwrap();
    thread::sleep(Duration::from_millis(50));

    // Client connects
    let mut client = IpcClient::connect_uds(&socket_path).unwrap();
    let welcome = client.handshake("test-frontend", "0.3.0").unwrap();
    assert!(matches!(welcome, DaemonMessage::Welcome { .. }));

    // Subscribe to CPU & MEMORY
    client
        .subscribe(SubscriptionMask::CPU | SubscriptionMask::MEMORY, 100)
        .unwrap();

    // Verify subscriber registered in service
    thread::sleep(Duration::from_millis(50));
    assert!(service.subscriber_count() >= 1);

    // Wait for at least one live TelemetryBatch broadcasted from the sampling loop
    let mut received_batch = false;
    for _ in 0..10 {
        if let Ok(DaemonMessage::TelemetryBatch {
            timestamp_ms,
            facet_mask,
            payload_json,
        }) = client.receive_daemon_message()
        {
            assert!(timestamp_ms > 0);
            assert_eq!(facet_mask, SubscriptionMask::ALL.bits());
            assert!(!payload_json.is_empty());
            received_batch = true;
            break;
        }
    }
    assert!(
        received_batch,
        "Client must receive live TelemetryBatch broadcast"
    );

    service.stop();
    let _ = join_handle.join();
}

#[cfg(not(windows))]
#[test]
fn e2e_daemon_real_history_query_and_response() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("history_daemon.sock");
    let history_dir = temp_dir.path().join("query_history");

    let history = DaemonHistoryManager::open(&history_dir).unwrap();
    // Record real sample at current wall clock to match TTL retention window
    let now = unix_millis(std::time::SystemTime::now());
    let key = HistorySeriesKey::system(HistoryMetric::CpuUsagePct);
    let sample = HistoricalSample {
        revision: 1,
        completed_at_ms: now,
        measured_at_ms: Some(now),
        value: Some(33.3),
    };
    history.record_sample(key, sample).unwrap();
    history.flush(now).unwrap();

    let config = DaemonConfig::new(IpcEndpoint::unix(&socket_path), &history_dir);
    let service = Arc::new(DaemonService::new(config, history));
    let join_handle = Arc::clone(&service).start_on_thread().unwrap();
    thread::sleep(Duration::from_millis(50));

    let mut client = IpcClient::connect_uds(&socket_path).unwrap();
    client.handshake("test-frontend", "0.3.0").unwrap();

    // Query history for CPU in range around `now`
    let query_msg = ClientMessage::QueryHistory {
        facet_mask: SubscriptionMask::CPU.bits(),
        range_start_ms: now.saturating_sub(5000),
        range_end_ms: now.saturating_add(5000),
    };
    client.send_client_message(&query_msg).unwrap();

    let resp = client.receive_daemon_message().unwrap();
    if let DaemonMessage::HistoryResponse {
        facet_mask,
        points_count,
        payload_json,
    } = resp
    {
        assert_eq!(facet_mask, SubscriptionMask::CPU.bits());
        assert_eq!(points_count, 1);
        assert!(payload_json.contains("33.3"));
    } else {
        panic!("Expected HistoryResponse, got {resp:?}");
    }

    service.stop();
    let _ = join_handle.join();
}
