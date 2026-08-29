use super::*;

#[test]
fn disabled_frontend_history_is_inert_before_any_path_or_worker_lookup() {
    let root = fixture_root("disabled");
    let host = fixture_host(root.join("config.json"), root.join("history"));
    let _connector = host
        .history_frontend_connector()
        .expect("control connector starts without touching history storage");
    assert!(host.history_replay_runtime.get().is_none());
    assert!(!root.exists());
}

#[test]
fn connector_submission_is_bounded_and_request_ids_are_monotonic() {
    let (request_tx, request_rx) = std::sync::mpsc::sync_channel(2);
    let (_completion_tx, completion_rx) = std::sync::mpsc::sync_channel(1);
    let mut connector = HistoryFrontendConnector {
        requests: request_tx,
        completions: completion_rx,
        next_request: Some(1),
    };

    let first = connector.try_connect().expect("first request");
    let second = connector.try_connect().expect("second request");
    assert_ne!(first, second);
    assert_eq!(
        connector.try_connect(),
        Err(HistoryFrontendConnectSubmitError::Busy)
    );
    assert_eq!(request_rx.try_recv().expect("queued first").id, first);
    assert_eq!(request_rx.try_recv().expect("queued second").id, second);
}

fn fixture_root(tag: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(".tmp")
        .join(format!(
            "taskforest-history-frontend-{}-{tag}",
            std::process::id()
        ))
}

fn fixture_host(
    config_path: std::path::PathBuf,
    history_root: std::path::PathBuf,
) -> NativeAppHost {
    NativeAppHost {
        config_path,
        history_root,
        local_time_cache: std::sync::Arc::new(crate::StartupLocalTimeCache::capture(
            taskmanager_core::core::time::LocalTimeRulesObservation::unsupported(0),
        )),
        config_runtime: std::sync::Arc::new(std::sync::OnceLock::new()),
        history_replay_runtime: std::sync::Arc::new(std::sync::OnceLock::new()),
        history_persistence_runtime: std::sync::Arc::new(std::sync::OnceLock::new()),
        snapshot_export_runtime: std::sync::Arc::new(std::sync::OnceLock::new()),
        diagnostic_bundle_runtime: std::sync::Arc::new(std::sync::OnceLock::new()),
    }
}
