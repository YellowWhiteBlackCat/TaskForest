//! Terminal-cell text geometry.
//!
//! Rust string length is not terminal width: wide CJK graphemes occupy two
//! cells, combining marks occupy zero, and a grapheme must never be cut in the
//! middle. Ratatui already carries the Unicode-width/grapheme machinery used
//! by its renderer; this module keeps all product-side padding and truncation
//! at that same cell boundary.

use ratatui::style::Style;
use ratatui::text::Line;

/// Measure the number of terminal cells occupied by one text value.
#[must_use]
pub(super) fn cell_width(value: &str) -> usize {
    Line::from(value).width()
}

/// Pad a value to at least `width` terminal cells without splitting a
/// grapheme. Values wider than the requested width are preserved; callers
/// that need a hard bound should truncate first.
#[must_use]
pub(super) fn pad_cells(value: &str, width: usize) -> String {
    let mut output = value.to_owned();
    output.extend(std::iter::repeat_n(
        ' ',
        width.saturating_sub(cell_width(value)),
    ));
    output
}

/// Truncate a value to at most `width` cells, appending an ellipsis when the
/// value does not fit. The cut happens between Ratatui graphemes, never in the
/// middle of a UTF-8 sequence or a combining/emoji cluster.
#[must_use]
pub(super) fn truncate_cells(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if cell_width(value) <= width {
        return value.to_owned();
    }
    let ellipsis = "…";
    let ellipsis_width = cell_width(ellipsis).min(width);
    if ellipsis_width == width {
        return ellipsis.to_owned();
    }

    let budget = width - ellipsis_width;
    let line = Line::from(value);
    let mut output = String::new();
    let mut used: usize = 0;
    for grapheme in line.styled_graphemes(Style::default()) {
        let grapheme_width = cell_width(grapheme.symbol);
        if used.saturating_add(grapheme_width) > budget {
            break;
        }
        output.push_str(grapheme.symbol);
        used = used.saturating_add(grapheme_width);
    }
    output.push_str(ellipsis);
    output
}

/// Preserve the tail of a value while bounding it to terminal cells. This is
/// used for feedback where the actionable error detail is at the end.
#[must_use]
pub(super) fn truncate_tail_cells(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if cell_width(value) <= width {
        return value.to_owned();
    }
    let ellipsis = "…";
    let ellipsis_width = cell_width(ellipsis).min(width);
    if ellipsis_width == width {
        return ellipsis.to_owned();
    }

    let budget = width - ellipsis_width;
    let line = Line::from(value);
    let graphemes: Vec<&str> = line
        .styled_graphemes(Style::default())
        .map(|grapheme| grapheme.symbol)
        .collect();
    let mut tail = Vec::new();
    let mut used: usize = 0;
    for grapheme in graphemes.iter().rev() {
        let grapheme_width = cell_width(grapheme);
        if used.saturating_add(grapheme_width) > budget {
            break;
        }
        tail.push(*grapheme);
        used = used.saturating_add(grapheme_width);
    }
    let mut output = String::from(ellipsis);
    for grapheme in tail.into_iter().rev() {
        output.push_str(grapheme);
    }
    output
}

#[cfg(test)]
#[path = "../../tests/gui/ui/text_tests.rs"]
mod tests;
