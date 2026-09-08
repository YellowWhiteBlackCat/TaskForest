//! test-intent: behavior
//!
//! Pure parser coverage for the bounded macOS `nettop` process-rate adapter.

use super::*;

#[test]
fn nettop_csv_uses_headers_and_keeps_process_rates_separate() {
    let output =
        "pid,process,bytes_out,bytes_in\n42,worker,8192,4096\n43,quoted,not-a-number,1024\n";
    let facts = parse_nettop_csv(output);
    assert_eq!(facts.get(&42), Some(&(Some(4096), Some(8192))));
    assert_eq!(facts.get(&43), Some(&(Some(1024), None)));
}

#[test]
fn malformed_or_empty_nettop_rows_do_not_become_zero_rates() {
    let output = "pid,bytes_in,bytes_out\nnot-a-pid,0,0\n44,-,-\n";
    assert!(parse_nettop_csv(output).is_empty());
    assert!(parse_nettop_csv("").is_empty());
}
