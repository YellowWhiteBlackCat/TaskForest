//! test-intent: integration
//!
//! Behavior tests for daemon service lifecycle, state machine, and persistence integration.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use taskmanager_core::{HistoricalSample, HistoryMetric, HistorySeriesKey};
use taskmanager_daemon::collector::DaemonCollector;
use taskmanager_daemon::config::DaemonConfig;
use taskmanager_daemon::persistence::DaemonHistoryManager;
use taskmanager_daemon::service::DaemonService;
use taskmanager_daemon::state::{DaemonCadenceMode, DaemonState};
use taskmanager_ipc::protocol::{DaemonMessage, SubscriptionMask};
use taskmanager_ipc::service::{ClientSession, IpcClient};
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
