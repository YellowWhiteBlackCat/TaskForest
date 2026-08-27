use super::{
    ProcessBatchHistory, ProcessBatchHistoryFormat, ProcessBatchResult, ProcessBatchTargetResult,
    export_process_batch_history,
};
use crate::core::process::{FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent};

fn intent() -> ProcessBatchIntent {
    ProcessBatchIntent {
        action: ProcessBatchAction::End,
        scope: Default::default(),
        targets: vec![
            FrozenProcessIdentity::from_authoritative_parts(101, "alpha", 1, 1).expect("fixture"),
            FrozenProcessIdentity::from_authoritative_parts(202, "beta", 2, 2).expect("fixture"),
        ],
    }
}

fn result() -> ProcessBatchResult {
    ProcessBatchResult {
        intent: intent(),
        targets: vec![
            (
                FrozenProcessIdentity::from_authoritative_parts(101, "alpha", 1, 1)
                    .expect("fixture"),
                ProcessBatchTargetResult::Applied,
            ),
            (
                FrozenProcessIdentity::from_authoritative_parts(202, "beta", 2, 2)
                    .expect("fixture"),
                ProcessBatchTargetResult::IdentityChanged,
            ),
        ],
    }
}

#[test]
fn capacity_and_emptiness_reflect_recorded_entries() {
    let mut history = ProcessBatchHistory::new(2);
    assert!(history.is_empty());
    assert_eq!(history.capacity(), 2);

    history.record_result(1_000, result());
    assert!(!history.is_empty());
    assert_eq!(history.len(), 1);
    assert_eq!(history.capacity(), 2, "capacity is stable, entries are not");
}

#[test]
fn csv_rows_number_targets_from_one_with_no_leading_comma() {
    let mut history = ProcessBatchHistory::new(8);
    history.record_result(1_720_000_000, result());
    let csv =
        export_process_batch_history(&history, ProcessBatchHistoryFormat::Csv).expect("csv export");

    let lines: Vec<&str> = csv.lines().collect();
    assert!(lines[0].starts_with("schema_version,"), "header first");
    assert_eq!(lines.len(), 3, "header + 2 target rows");
    // target_index starts at 1 (a `+`→`*` mutation of `index + 1` would
    // print 0 for the first target). Row shape:
    // schema,completed,action,priority,target_index,target_count,pid,name,...
    assert!(
        lines[1].starts_with("1,1720000000,end,") && lines[1].contains(",1,2,101,alpha,"),
        "first row: index 1, pid 101; got: {}",
        lines[1]
    );
    assert!(
        lines[2].contains(",2,2,202,beta,"),
        "second row: index 2, pid 202; got: {}",
        lines[2]
    );
    // No row may begin with a comma (a `>`→`>=` separator mutation puts
    // a comma before the first field).
    for line in &lines[1..] {
        assert!(
            !line.starts_with(','),
            "row must not start with a separator: {line}"
        );
    }
}

#[test]
fn export_error_display_and_source_are_meaningful() {
    // The error wraps a serde_json failure; both Display and source()
    // must surface it (constant-true/false mutations of either are
    // caught by the non-empty / Some assertions).
    let inner = serde_json::from_str::<u8>("not-a-number").expect_err("invalid json");
    let error = super::ProcessBatchHistoryExportError(inner);
    assert!(!error.to_string().is_empty());
    assert!(
        std::error::Error::source(&error).is_some(),
        "export error must chain its cause"
    );
}
