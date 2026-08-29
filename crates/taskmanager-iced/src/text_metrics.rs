//! Measured text geometry for canvas-drawn surfaces (chart readout pills,
//! axis labels, hover badges).
//!
//! `canvas::Text` paints shaped text but the canvas frame exposes no
//! metrics, so badge geometry had to estimate width from character counts —
//! wrong for proportional glyphs and CJK. This helper shapes the same text
//! through the paragraph layer and returns the measured extent. It needs no
//! live renderer (the paragraph layer owns its shaping stack), so chart
//! caches can call it at geometry-build time.

/// The measured on-screen width of one single-line text run at `size`,
/// through the same shaping the renderer paints with.
#[must_use]
pub(crate) fn measured_text_width(content: &str, size: f32) -> f32 {
    use iced::Size;
    use iced::advanced::text::{self, Wrapping, paragraph::Plain};

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

#[cfg(test)]
#[path = "../tests/gui/text_metrics_tests.rs"]
mod tests;
