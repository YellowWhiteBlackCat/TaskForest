//! Measured text geometry for canvas-drawn surfaces (chart readout pills,
//! axis labels, hover badges) and for text selection.
//!
//! `canvas::Text` paints shaped text but the canvas frame exposes no
//! metrics, so badge geometry had to estimate width from character counts —
//! wrong for proportional glyphs and CJK. This helper shapes the same text
//! through the paragraph layer and returns the measured extent. It needs no
//! live renderer (the paragraph layer owns its shaping stack), so chart
//! caches can call it at geometry-build time.
//!
//! The same paragraph layer is the only honest source of *line* geometry:
//! a wrapped run's line bands, per-line byte ranges and per-grapheme caret
//! positions all come from the shaped buffer, never from an estimated
//! character count. Multi-line selection is built on this seam, so a
//! selection rectangle is measured geometry instead of a guess.

use iced::advanced::text::paragraph::Plain;
use iced::advanced::text::{self, Paragraph, Wrapping};
use unicode_segmentation::UnicodeSegmentation;

/// The measured on-screen width of one single-line text run at `size`,
/// through the same shaping the renderer paints with.
#[must_use]
pub(crate) fn measured_text_width(content: &str, size: f32) -> f32 {
    use iced::Size;

    let mut paragraph = Plain::<<iced::Renderer as text::Renderer>::Paragraph>::default();
    let _ = paragraph.update(text::Text {
        content,
        bounds: Size::new(f32::INFINITY, f32::INFINITY),
        size: size.into(),
        line_height: text::LineHeight::default(),
        font: iced::Font::default(),
        align_x: text::Alignment::Default,
        align_y: iced::alignment::Vertical::Top,
        shaping: text::Shaping::Auto,
        wrapping: Wrapping::None,
    });
    paragraph.min_bounds().width
}

/// The longest prefix of `content` that measures within `max_width`, plus a
/// trailing ellipsis when truncation happened. `measure` is injected so the
/// prefix search is provable headlessly with a synthetic width function.
fn truncate_by_measure(content: &str, max_width: f32, measure: &dyn Fn(&str) -> f32) -> String {
    if measure(content) <= max_width {
        return content.to_owned();
    }
    let ellipsis_w = measure("…");
    let budget = (max_width - ellipsis_w).max(0.0);
    let mut best = String::new();
    for (index, _) in content.char_indices().skip(1) {
        let prefix = &content[..index];
        if measure(prefix) > budget {
            break;
        }
        best.clear();
        best.push_str(prefix);
    }
    if best.is_empty() {
        return String::new();
    }
    format!("{best}…")
}

/// Truncate one single-line run to the measured `max_width` with a trailing
/// ellipsis (GPUI truncation parity: an ellipsized prefix, never a hard
/// mid-glyph clip). Char-boundary safe; text that fits is returned unchanged.
#[must_use]
pub(crate) fn truncate_to_width(content: &str, max_width: f32, size: f32) -> String {
    truncate_by_measure(content, max_width, &|s| measured_text_width(s, size))
}

/// One laid-out visual line of a wrapped text run.
///
/// Every field is measured from the shaped paragraph the renderer paints
/// with: the band comes from the line's glyph run, the byte range from the
/// run's own glyph coverage, and the carets from the shaped advance
/// positions. Nothing here is estimated from character counts.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeasuredLine {
    /// Layout-run index of the line inside the paragraph.
    pub line: usize,
    /// Byte offset of the line's own text inside the measured content.
    pub start_byte: usize,
    /// Byte offset one past the line's own text.
    pub end_byte: usize,
    /// Vertical band of the line, relative to the paragraph origin.
    pub top: f32,
    /// Height of the line band.
    pub height: f32,
    /// Horizontal band of the line's text, relative to the paragraph origin.
    pub left: f32,
    /// Right edge of the line's text band.
    pub right: f32,
    /// Caret x of every grapheme boundary on this line. The length is the
    /// line's grapheme count plus one (the trailing caret sits at the line
    /// end).
    pub carets: Vec<f32>,
}

impl MeasuredLine {
    /// The grapheme index of `byte` inside this line, counted from the
    /// line's own start. A byte outside the line clamps to the line's edge.
    pub fn grapheme_index_at(&self, content: &str, byte: usize) -> usize {
        let clamped = byte.clamp(self.start_byte, self.end_byte);
        content
            .get(self.start_byte..clamped)
            .map(|prefix| prefix.graphemes(true).count())
            .unwrap_or(0)
    }
}

/// The byte offset of every original line inside `content`.
///
/// cosmic-text reports a layout run's text as a slice of the *original*
/// line, so a run's byte range is resolved through the original line's own
/// offset in the content.
fn original_line_starts(content: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (index, byte) in content.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

/// A shaped paragraph together with its measured line geometry.
///
/// The paragraph is the same one the renderer paints with, so the line
/// bands, the per-line byte ranges and the per-grapheme caret positions are
/// measured geometry. Owning both in one value keeps a widget from shaping
/// the same text twice, and keeps the line geometry and the painted text
/// from ever drifting apart.
#[derive(Default)]
pub(crate) struct MeasuredParagraph {
    paragraph: Plain<<iced::Renderer as text::Renderer>::Paragraph>,
    lines: Vec<MeasuredLine>,
}

impl MeasuredParagraph {
    /// Shape `content` at `size` inside `bounds` with the given wrapping,
    /// and re-measure the line geometry when the shape actually changed.
    pub(crate) fn update(
        &mut self,
        content: &str,
        size: f32,
        bounds: iced::Size,
        wrapping: iced::advanced::text::Wrapping,
    ) {
        let changed = self.paragraph.update(text::Text {
            content,
            bounds,
            size: size.into(),
            line_height: text::LineHeight::default(),
            font: iced::Font::default(),
            align_x: text::Alignment::Default,
            align_y: iced::alignment::Vertical::Top,
            shaping: text::Shaping::Auto,
            wrapping,
        });
        if changed {
            self.lines = measure_lines(content, &self.paragraph);
        }
    }

    /// The shaped paragraph the renderer paints with.
    pub(crate) fn paragraph(&self) -> &Plain<<iced::Renderer as text::Renderer>::Paragraph> {
        &self.paragraph
    }

    /// The measured line geometry of the shaped content.
    pub(crate) fn lines(&self) -> &[MeasuredLine] {
        &self.lines
    }
}

/// The measured line geometry of a shaped paragraph.
fn measure_lines(
    content: &str,
    paragraph: &Plain<<iced::Renderer as text::Renderer>::Paragraph>,
) -> Vec<MeasuredLine> {
    let raw = paragraph.raw();
    let bands = raw.span_bounds(0);
    let original_starts = original_line_starts(content);

    let mut lines = Vec::new();
    for (index, run) in raw.buffer().layout_runs().enumerate() {
        let Some(band) = bands.get(index) else {
            break;
        };
        let Some(base) = original_starts.get(run.line_i).copied() else {
            break;
        };
        let (Some(first), Some(last)) = (run.glyphs.first(), run.glyphs.last()) else {
            continue;
        };
        let start_byte = base + first.start;
        let end_byte = base + last.end;
        let graphemes = content
            .get(start_byte..end_byte)
            .map(|segment| segment.graphemes(true).count())
            .unwrap_or(0);

        let mut carets = Vec::with_capacity(graphemes + 1);
        for grapheme in 0..=graphemes {
            carets.push(
                raw.grapheme_position(index, grapheme)
                    .map(|point| point.x)
                    .unwrap_or(band.x + band.width),
            );
        }

        lines.push(MeasuredLine {
            line: index,
            start_byte,
            end_byte,
            top: band.y,
            height: band.height,
            left: band.x,
            right: band.x + band.width,
            carets,
        });
    }
    lines
}

/// The index of the line whose vertical band contains `y`.
///
/// A point above the first band resolves to the first line and a point past
/// the last band resolves to the last line: a drag that runs off the text
/// must still land on a real line instead of losing the caret.
pub(crate) fn line_at_y(lines: &[MeasuredLine], y: f32) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }
    for (index, line) in lines.iter().enumerate() {
        if y < line.top + line.height {
            return Some(index);
        }
    }
    Some(lines.len() - 1)
}

#[cfg(test)]
#[path = "../tests/gui/text_metrics_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/gui/text_metrics_lines_tests.rs"]
mod lines_tests;
