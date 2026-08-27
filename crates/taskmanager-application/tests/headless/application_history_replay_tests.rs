use std::sync::Arc;

use taskmanager_application::{
    HistoryReplayCompletion, HistoryReplayCompletionDisposition, HistoryReplayCompletionOutcome,
    HistoryReplayController, HistoryReplayError, HistoryReplayErrorKind, HistoryReplayRow,
    HistoryReplayState, HistorySeriesKey, HistoryWindow,
};
use taskmanager_core::HistoryMetric;

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
    assert_eq!(
        replay.refresh(),
        Err(taskmanager_application::HistoryReplayTransitionError::Closed)
    );
    assert_eq!(
        replay.select_window(HistoryWindow::TwentyFourHours),
        Err(taskmanager_application::HistoryReplayTransitionError::Closed)
    );
    assert_eq!(replay.selected_window(), HistoryWindow::OneHour);

    let error = HistoryReplayError::new(
        HistoryReplayErrorKind::Read,
        "界".repeat(taskmanager_application::MAX_HISTORY_REPLAY_ERROR_CHARS + 5),
    );
    assert_eq!(
        error.detail().chars().count(),
        taskmanager_application::MAX_HISTORY_REPLAY_ERROR_CHARS
    );
}
