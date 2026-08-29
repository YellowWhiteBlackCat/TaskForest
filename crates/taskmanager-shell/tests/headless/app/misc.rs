//! Misc shared-shell tests: typed notification-failure reporting and the
//! persisted history-capacity pass-through (G-02).

use taskmanager_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, ScalarObservation, SystemSnapshot,
};

fn snapshot_with_cpu(cpu_usage: f32, timestamp_ms: u64) -> SystemSnapshot {
    SystemSnapshot {
        timestamp_ms,
        cpu: CpuMetrics::from_observations(CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(cpu_usage, timestamp_ms),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn desktop_notification_submission_failure_is_typed_in_the_status_line() {
    // BN-07 failure path: when the platform cannot deliver (no notification
    // lane / no DBus service), the shell reports a typed capability error —
    // never a fabricated success.
    let mut app = crate::demo_app();
    app.report_submission_error(&taskmanager_platform_contract::SubmissionError {
        capability: taskmanager_platform_contract::CapabilityId::DESKTOP_NOTIFY,
        kind: taskmanager_platform_contract::SubmissionErrorKind::UnsupportedCapability,
    });
    assert!(
        app.feedback_text().contains("alerts.notify"),
        "the typed capability must surface: {}",
        app.feedback_text()
    );
    assert!(
        app.feedback_text().contains("Unsupported"),
        "{}",
        app.feedback_text()
    );
}

/// G-02: the persisted `graph_data_points` preference reaches the shared
/// shell store through [`ShellApp::set_history_capacity`] — clamped to the
/// product's 10..=600 window range, preserving the newest samples, so every
/// frontend reading the shared series sees the same window it configured.
#[test]
fn set_history_capacity_passes_through_to_the_shared_history_store() {
    let mut app = crate::demo_app();
    // Default construction keeps the legacy 64-sample window until a
    // frontend applies the preference.
    assert_eq!(
        app.history.capacity(),
        taskmanager_telemetry_store::live_graph::DEFAULT_HISTORY_CAPACITY
    );
    for tick in 0..20u64 {
        let snapshot = snapshot_with_cpu(tick as f32 + 1.0, tick + 1);
        crate::fixture::record_demo_history_frame(&mut app, &snapshot, None, None);
    }
    // An out-of-range request clamps; the shared window keeps the NEWEST ten.
    app.set_history_capacity(0);
    assert_eq!(
        app.history.capacity(),
        taskmanager_telemetry_store::live_graph::MIN_HISTORY_CAPACITY
    );
    let series = app
        .history
        .series(taskmanager_telemetry_store::live_graph::MetricSeries::CpuUsagePercent);
    assert_eq!(
        series,
        (11..=20).map(|value| value as f32).collect::<Vec<_>>(),
        "the newest samples survive the resize"
    );
    app.set_history_capacity(usize::MAX);
    assert_eq!(
        app.history.capacity(),
        taskmanager_telemetry_store::live_graph::MAX_HISTORY_CAPACITY
    );
}

/// The allocation-free index/read helpers must expose the same visible order
/// and PID lookup as the legacy borrowed-row vector used by action paths.
#[test]
fn visible_process_index_helpers_match_the_borrowed_projection() {
    let app = crate::demo_app();
    let legacy = app
        .visible_processes()
        .iter()
        .map(|process| process.pid)
        .collect::<Vec<_>>();
    let source = app.data.processes.as_deref().unwrap_or_default();
    let indices = app.visible_process_indices();
    let direct = indices
        .iter()
        .filter_map(|&index| source.get(index))
        .map(|process| process.pid)
        .collect::<Vec<_>>();

    assert_eq!(direct, legacy);
    for (index, pid) in legacy.iter().copied().enumerate() {
        assert_eq!(
            app.visible_process_at(index).map(|process| process.pid),
            Some(pid)
        );
        assert_eq!(app.visible_process_index_of_pid(pid), Some(index));
        assert_eq!(
            app.visible_process_by_pid(pid).map(|process| process.pid),
            Some(pid)
        );
    }
    assert_eq!(app.visible_process_count(), legacy.len());
}
