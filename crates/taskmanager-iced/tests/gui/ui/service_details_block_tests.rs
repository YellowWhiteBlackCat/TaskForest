//! Headless behavior tests for the service-log whole-block selection
//! (ICED-043): the visible log lines are ONE multi-line value, and the
//! shared measured paragraph seam resolves it into one measured line per
//! log line — so a drag selection can span the lines it covers.

use super::*;
use crate::text_metrics::MeasuredParagraph;
use iced::Size;
use iced::advanced::text::Wrapping;

fn log_lines() -> Vec<String> {
    vec![
        "systemd: Starting the service".to_owned(),
        "systemd: Started the service".to_owned(),
        "kernel: transferred bytes".to_owned(),
    ]
}

/// The block carries every visible line, in order, separated by REAL line
/// breaks: the log text is never reworded, dropped or reordered.
#[test]
fn service_log_block_keeps_every_line_in_order() {
    let lines = log_lines();
    let block = service_log_block(&lines);
    assert_eq!(
        block,
        "systemd: Starting the service\nsystemd: Started the service\nkernel: transferred bytes"
    );
    assert_eq!(
        block.lines().count(),
        lines.len(),
        "the block carries one line per visible log line"
    );
}

/// The shared measured seam resolves the block into ONE measured line per
/// log line: the geometry a cross-line selection needs is measured, not
/// estimated.
#[test]
fn measured_paragraph_resolves_the_block_into_one_line_per_log_line() {
    let lines = log_lines();
    let block = service_log_block(&lines);
    let mut measured = MeasuredParagraph::default();
    measured.update(&block, 11.0, Size::new(600.0, 400.0), Wrapping::Word);
    let measured_lines = measured.lines();
    assert_eq!(
        measured_lines.len(),
        lines.len(),
        "a drag can only span lines the paragraph actually measures"
    );
    // The measured byte ranges run over the block in order, so a selection
    // anchored in the first line and dragged into the last covers a real
    // multi-line byte range.
    assert_eq!(measured_lines[0].start_byte, 0);
    let last = measured_lines
        .last()
        .expect("the block measures at least one line");
    assert_eq!(last.end_byte, block.len());
    for pair in measured_lines.windows(2) {
        assert!(
            pair[0].end_byte <= pair[1].start_byte,
            "the measured lines stay ordered and disjoint"
        );
    }
}

/// Every measured line carries a real caret run, so a selection inside one
/// line still resolves to grapheme boundaries.
#[test]
fn measured_lines_carry_real_caret_positions() {
    let block = service_log_block(&log_lines());
    let mut measured = MeasuredParagraph::default();
    measured.update(&block, 11.0, Size::new(600.0, 400.0), Wrapping::Word);
    for line in measured.lines() {
        assert!(
            line.carets.len() >= 2,
            "a measured line carries a caret per grapheme boundary plus its end"
        );
        assert!(
            line.carets.windows(2).all(|pair| pair[0] <= pair[1]),
            "the carets advance monotonically along the line"
        );
    }
}

/// An empty log block stays empty: no placeholder, no fabricated line.
#[test]
fn empty_log_block_stays_empty() {
    assert_eq!(service_log_block(&[]), "");
}
