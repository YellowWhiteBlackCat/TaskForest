use std::sync::Arc;

use taskmanager_application::HistoryReplayTransitionError;
use taskmanager_application::MAX_HISTORY_REPLAY_ERROR_CHARS;
use taskmanager_application::{
    HistoryReplayCompletion, HistoryReplayCompletionDisposition, HistoryReplayCompletionOutcome,
    HistoryReplayController, HistoryReplayError, HistoryReplayErrorKind, HistoryReplayRow,
    HistoryReplayState,
};
use taskmanager_core::core::history::{HistoryMetric, HistorySeriesKey, HistoryWindow};

fn rows(value: f32) -> Arc<[HistoryReplayRow]> {
    Arc::from([HistoryReplayRow {
        key: HistorySeriesKey::system(HistoryMetric::CpuUsagePct),
        samples: Arc::from([value]),
        sample_times_ms: Arc::from([1_000]),
        peak_value: Some(f64::from(value)),
        peak_measured_at_ms: Some(1_000),
        observed: 1,
        gaps: 0,
        clock_jumps: 0,
    }])
}

#[test]
fn late_completion_cannot_replace_the_current_request() {
    let mut replay = HistoryReplayController::default();
    let first = replay.open().expect("open replay");
    let current = replay
        .select_window(HistoryWindow::SevenDays)
        .expect("change an open replay window");

    assert_eq!(
        replay.complete(HistoryReplayCompletion {
            request: first,
            loaded_at_ms: 1_000,
            outcome: HistoryReplayCompletionOutcome::Loaded(rows(12.0)),
        }),
        HistoryReplayCompletionDisposition::StaleIgnored
    );
    assert!(matches!(
        replay.state(),
        HistoryReplayState::Loading { request, .. } if *request == current
    ));

    assert_eq!(
        replay.complete(HistoryReplayCompletion {
            request: current,
            loaded_at_ms: 2_000,
            outcome: HistoryReplayCompletionOutcome::Loaded(rows(24.0)),
        }),
        HistoryReplayCompletionDisposition::Applied
    );
    assert_eq!(replay.rows()[0].peak_value, Some(24.0));
    assert_eq!(replay.loaded_at_ms(), Some(2_000));
}

#[test]
fn failed_refresh_retains_last_good_evidence_and_close_is_terminal() {
    let mut replay = HistoryReplayController::default();
    let loaded = replay.open().expect("open replay");
    assert_eq!(
        replay.complete(HistoryReplayCompletion {
            request: loaded,
            loaded_at_ms: 4_000,
            outcome: HistoryReplayCompletionOutcome::Loaded(rows(41.0)),
        }),
        HistoryReplayCompletionDisposition::Applied
    );

    let refresh = replay
        .select_window(HistoryWindow::SevenDays)
        .expect("request a wider replay window");
    assert_eq!(replay.rows()[0].peak_value, Some(41.0));
    assert_eq!(replay.rows_window(), Some(HistoryWindow::OneHour));
    assert_eq!(replay.loaded_at_ms(), Some(4_000));
    assert_eq!(replay.rows_request_id(), Some(loaded.id()));
    let failure = HistoryReplayError::new(HistoryReplayErrorKind::Read, "fixture read failed");
    assert_eq!(
        replay.reject_submission(refresh, failure.clone()),
        HistoryReplayCompletionDisposition::Applied
    );
    assert_eq!(replay.failure(), Some(&failure));
    assert_eq!(replay.rows()[0].peak_value, Some(41.0));
    assert_eq!(replay.loaded_at_ms(), Some(4_000));
    assert_eq!(replay.rows_window(), Some(HistoryWindow::OneHour));
    assert_eq!(replay.selected_window(), HistoryWindow::SevenDays);

    replay.close();
    assert_eq!(replay.state(), &HistoryReplayState::Closed);
    assert_eq!(
        replay.complete(HistoryReplayCompletion {
            request: refresh,
            loaded_at_ms: 5_000,
            outcome: HistoryReplayCompletionOutcome::Loaded(rows(99.0)),
        }),
        HistoryReplayCompletionDisposition::StaleIgnored
    );
    assert_eq!(replay.state(), &HistoryReplayState::Closed);

    let reopened = replay.open().expect("open a new replay session");
    assert!(matches!(
        replay.state(),
        HistoryReplayState::Loading {
            request,
            last_good: None,
        } if *request == reopened
    ));
    assert!(replay.rows().is_empty());
    assert_eq!(replay.loaded_at_ms(), None);
    assert_eq!(replay.rows_request_id(), None);
}

#[test]
fn closed_controller_rejects_refresh_and_window_selection() {
    let mut replay = HistoryReplayController::default();
    assert_eq!(replay.refresh(), Err(HistoryReplayTransitionError::Closed));
    assert_eq!(
        replay.select_window(HistoryWindow::TwentyFourHours),
        Err(HistoryReplayTransitionError::Closed)
    );
    assert_eq!(replay.selected_window(), HistoryWindow::OneHour);

    let error = HistoryReplayError::new(
        HistoryReplayErrorKind::Read,
        "界".repeat(MAX_HISTORY_REPLAY_ERROR_CHARS + 5),
    );
    assert_eq!(
        error.detail().chars().count(),
        MAX_HISTORY_REPLAY_ERROR_CHARS
    );
}

#[test]
fn performance_projection_filters_application_rows_and_keeps_last_good_request_identity() {
    use taskmanager_application::{ApplicationHistoryCapability, ApplicationHistoryStatus};
    use taskmanager_core::core::history::ApplicationHistoryIdentity;
    let mut replay = HistoryReplayController::default();
    let request = replay.open().expect("open");
    let system = rows(12.0)[0].clone();
    let mut application = system.clone();
    application.key = HistorySeriesKey::for_application(
        HistoryMetric::ApplicationCpuUsagePct,
        ApplicationHistoryIdentity::verified_launcher("io.example.Reader").expect("identity"),
    );
    let _ = replay.complete(HistoryReplayCompletion {
        request,
        loaded_at_ms: 1_000,
        outcome: HistoryReplayCompletionOutcome::Loaded(Arc::from([system, application])),
    });
    let initial = replay.performance_history_projection(ApplicationHistoryCapability::Available);
    assert_eq!(initial.status, ApplicationHistoryStatus::Ready);
    assert_eq!(initial.rows.len(), 1);
    assert!(!initial.rows[0].key.is_application_series());
    assert_eq!(
        replay
            .application_history_projection(ApplicationHistoryCapability::Available)
            .rows
            .len(),
        1
    );
    let current = replay
        .select_window(HistoryWindow::TwentyFourHours)
        .expect("select");
    let refreshing = replay.performance_history_projection(ApplicationHistoryCapability::Available);
    assert!(refreshing.stale());
    assert_eq!(refreshing.rows_window, Some(HistoryWindow::OneHour));
    assert!(Arc::ptr_eq(&initial.rows, &refreshing.rows));
    assert_eq!(refreshing.source_request, Some(request.id()));
    assert_eq!(
        replay.complete(HistoryReplayCompletion {
            request,
            loaded_at_ms: 2_000,
            outcome: HistoryReplayCompletionOutcome::Loaded(rows(99.0))
        }),
        HistoryReplayCompletionDisposition::StaleIgnored
    );
    let _ = replay.complete(HistoryReplayCompletion {
        request: current,
        loaded_at_ms: 3_000,
        outcome: HistoryReplayCompletionOutcome::Loaded(rows(24.0)),
    });
    let next = replay.performance_history_projection(ApplicationHistoryCapability::Available);
    assert!(!next.stale());
    assert_eq!(next.rows[0].samples[0], 24.0);
    replay.close();
    let closed = replay.performance_history_projection(ApplicationHistoryCapability::Disabled);
    assert_eq!(closed.status, ApplicationHistoryStatus::Disabled);
    assert!(closed.rows.is_empty());
}

#[test]
fn replay_gap_projection_preserves_downtime_without_inventing_gaps_in_downsampled_windows() {
    let mut row = rows(0.0)[0].clone();
    row.samples = Arc::from([0.0, 10.0, 20.0, 30.0]);
    row.sample_times_ms = Arc::from([1_000, 2_000, 3_600_000, 3_601_000]);
    row.observed = 4;
    let samples = row.gap_aware_samples();
    assert_eq!(samples[0], 0.0, "measured zero remains a measurement");
    assert_eq!(samples.len(), 5);
    assert!(samples[2].is_nan());
    row.sample_times_ms = Arc::from([1_000, 301_000, 601_000, 901_000]);
    row.observed = 100_000;
    assert_eq!(
        &*row.gap_aware_samples(),
        &*row.samples,
        "regular downsampled points represent continuous dense records"
    );
}
