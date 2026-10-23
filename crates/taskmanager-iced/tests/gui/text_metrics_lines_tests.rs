// test-intent: behavior
//! The measured line-geometry contract behind multi-line text selection.
//!
//! A wrapped run is shaped by the same paragraph the renderer paints with, so
//! the line bands, the per-line byte ranges and the per-grapheme caret
//! positions are measured geometry. These tests assert the *structure* of
//! that geometry (line count, byte coverage, caret count, band ordering) and
//! never a host-specific pixel value: the exact widths belong to the font
//! and the host, the shape of the answer does not.

use super::{MeasuredParagraph, line_at_y};
use iced::Size;
use iced::advanced::text::Wrapping;
use unicode_segmentation::UnicodeSegmentation;

/// A long single-paragraph run that cannot fit on one narrow line.
const LONG_RUN: &str = "alpha bravo charlie delta echo foxtrot golf hotel india juliet \
kilo lima mike november oscar papa quebec romeo sierra tango";

/// The measured lines of `content` laid out at `size` inside `max_width`.
fn lines_of(content: &str, size: f32, max_width: f32) -> Vec<super::MeasuredLine> {
    let mut measured = MeasuredParagraph::default();
    measured.update(
        content,
        size,
        Size::new(max_width.max(1.0), f32::INFINITY),
        Wrapping::Word,
    );
    measured.lines().to_vec()
}

#[test]
fn an_empty_run_has_no_lines() {
    assert!(lines_of("", 14.0, 200.0).is_empty());
}

#[test]
fn a_run_that_fits_stays_on_one_line() {
    let lines = lines_of("alpha bravo", 14.0, 2000.0);

    assert_eq!(lines.len(), 1, "a run inside its budget is a single line");
}

#[test]
fn a_run_wider_than_its_budget_wraps_into_several_lines() {
    let lines = lines_of(LONG_RUN, 14.0, 120.0);

    assert!(
        lines.len() > 1,
        "a run wider than the budget must wrap, got {} line(s)",
        lines.len()
    );
}

#[test]
fn line_bands_are_ordered_and_disjoint() {
    let lines = lines_of(LONG_RUN, 14.0, 120.0);

    for pair in lines.windows(2) {
        assert!(
            pair[0].top + pair[0].height <= pair[1].top + f32::EPSILON,
            "line {} ends before line {} starts: {:?}",
            pair[0].line,
            pair[1].line,
            pair
        );
    }
}

#[test]
fn every_line_carries_exactly_one_caret_per_grapheme_boundary() {
    let content = "接口 if0 值";
    let lines = lines_of(content, 14.0, 2000.0);

    let line = &lines[0];
    let graphemes = content[line.start_byte..line.end_byte]
        .graphemes(true)
        .count();

    assert_eq!(
        line.carets.len(),
        graphemes + 1,
        "a line carries a caret at every grapheme boundary plus the line end"
    );
}

#[test]
fn carets_never_move_backwards_inside_a_line() {
    let lines = lines_of(LONG_RUN, 14.0, 120.0);

    for line in &lines {
        for pair in line.carets.windows(2) {
            assert!(
                pair[1] >= pair[0] - f32::EPSILON,
                "caret positions advance monotonically inside line {}: {:?}",
                line.line,
                line.carets
            );
        }
    }
}

#[test]
fn the_measured_lines_cover_the_whole_content() {
    let content = "first line\nsecond line\nthird line";
    let lines = lines_of(content, 14.0, 2000.0);

    assert_eq!(lines.len(), 3, "an explicit newline is a line break");

    for line in &lines {
        let segment = &content[line.start_byte..line.end_byte];
        assert!(
            !segment.is_empty(),
            "line {} owns a non-empty text segment",
            line.line
        );
    }

    let covered = lines
        .iter()
        .map(|line| &content[line.start_byte..line.end_byte])
        .collect::<Vec<_>>()
        .join(" ");
    let expected = content.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        covered, expected,
        "the measured segments reconstruct the content's text"
    );
}

#[test]
fn a_point_above_the_text_resolves_to_the_first_line() {
    let lines = lines_of(LONG_RUN, 14.0, 120.0);

    assert_eq!(line_at_y(&lines, -100.0), Some(0));
}

#[test]
fn a_point_below_the_text_resolves_to_the_last_line() {
    let lines = lines_of(LONG_RUN, 14.0, 120.0);
    let last = lines.len() - 1;

    assert_eq!(line_at_y(&lines, 10_000.0), Some(last));
}

#[test]
fn a_point_inside_a_band_resolves_to_that_line() {
    let lines = lines_of(LONG_RUN, 14.0, 120.0);
    let target = lines.len() / 2;
    let inside = lines[target].top + lines[target].height / 2.0;

    assert_eq!(line_at_y(&lines, inside), Some(target));
}
