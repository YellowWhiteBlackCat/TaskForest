use std::time::{Duration, Instant};

use zhuge_ui_core::frame_timing::record::FrameTimingIdentity;

use super::*;

fn sink_at(path: std::path::PathBuf) -> FrameTimingSink {
    FrameTimingSink::new(
        FrameTimingIdentity::new("taskforest", "taskforest-test", "test"),
        path,
        Vec::new(),
        240,
        16.0,
    )
}

fn sink_with_receipts(path: std::path::PathBuf) -> FrameTimingSink {
    FrameTimingSink::new(
        FrameTimingIdentity::new("taskforest", "taskforest-test", "test"),
        path,
        vec!["/tmp/soak-receipt".to_owned()],
        240,
        16.0,
    )
}

fn lines(path: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .expect("sink file exists")
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid JSONL record"))
        .collect()
}

#[test]
fn first_frame_reports_no_interval_and_stays_honest_about_execution() {
    let temp = tempfile::tempdir().expect("temp dir");
    let sink_path = temp.path().join("sink.jsonl");
    let mut sink = sink_at(sink_path.clone());

    sink.observe(
        Instant::now(),
        "first frame has no interval; test hook",
        "steady test hook",
    );

    let records = lines(&sink_path);
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record["frame_id"], 0);
    assert!(record["frame_interval_ms"].is_null());
    assert!(record["frame_execution_ms"].is_null());
    assert_eq!(record["reason"], "first frame has no interval; test hook");
    assert_eq!(record["stutter"], false);
    assert_eq!(record["capture_receipts"], serde_json::json!([]));
}

#[test]
fn steady_frame_carries_interval_reason_and_no_receipts_below_budget() {
    let temp = tempfile::tempdir().expect("temp dir");
    let sink_path = temp.path().join("sink.jsonl");
    let mut sink = sink_at(sink_path.clone());
    let start = Instant::now();
    sink.observe(start, "first", "steady");

    sink.observe(start + Duration::from_millis(8), "first", "steady");

    let records = lines(&sink_path);
    assert_eq!(records.len(), 2);
    let record = &records[1];
    assert_eq!(record["frame_id"], 1);
    let interval = record["frame_interval_ms"].as_f64().expect("interval");
    assert!((7.0..9.0).contains(&interval));
    assert_eq!(record["reason"], "steady");
    assert_eq!(record["stutter"], false);
    assert_eq!(record["capture_receipts"], serde_json::json!([]));
}

#[test]
fn stuttered_frame_binds_the_bound_receipts_and_counts_the_stutter() {
    let temp = tempfile::tempdir().expect("temp dir");
    let sink_path = temp.path().join("sink.jsonl");
    let mut sink = sink_with_receipts(sink_path.clone());
    let start = Instant::now();
    sink.observe(start, "first", "steady");

    sink.observe(start + Duration::from_millis(40), "first", "steady");

    let records = lines(&sink_path);
    assert_eq!(records.len(), 2);
    let record = &records[1];
    assert_eq!(record["stutter"], true);
    assert_eq!(record["stutter_count"], 1);
    assert_eq!(
        record["capture_receipts"],
        serde_json::json!(["/tmp/soak-receipt"])
    );
}
