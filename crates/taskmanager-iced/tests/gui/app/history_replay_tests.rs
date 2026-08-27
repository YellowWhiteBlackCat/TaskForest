use std::rc::Rc;
use std::sync::Arc;

use taskmanager_application::{
    HistoryMetric, HistoryReplayCompletion, HistoryReplayCompletionOutcome, HistoryReplayError,
    HistoryReplayErrorKind, HistoryReplayRow, HistorySeriesKey, HistoryWindow,
};

use super::*;

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
fn loading_and_failed_refresh_keep_the_labelled_last_good_graph_projection() {
    let mut replay = IcedHistoryReplay::default();
    let first = replay.open().expect("open replay");
    assert!(replay.complete(HistoryReplayCompletion {
        request: first,
        loaded_at_ms: 1_000,
        outcome: HistoryReplayCompletionOutcome::Loaded(rows(41.0)),
    }));
    let original_samples = Rc::as_ptr(&replay.rows()[0].samples);

    let wider = replay
        .select_window(HistoryWindow::SevenDays)
        .expect("select wider window");
    assert!(replay.is_loading());
    assert_eq!(replay.rows()[0].peak_value, Some(41.0));
    assert_eq!(replay.rows_window(), Some(HistoryWindow::OneHour));

    replay.reject_submission(
        wider,
        HistoryReplayError::new(HistoryReplayErrorKind::Read, "fixture failure"),
    );
    assert_eq!(replay.rows_window(), Some(HistoryWindow::OneHour));
    assert_eq!(replay.window(), HistoryWindow::SevenDays);
    assert_eq!(Rc::as_ptr(&replay.rows()[0].samples), original_samples);

    replay.close();
    assert_eq!(replay.rows()[0].peak_value, Some(41.0));
    assert!(
        replay.open().is_none(),
        "switching Performance presentation must not close the durable reader used by History"
    );
}
