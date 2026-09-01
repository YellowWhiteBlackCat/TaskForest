//! Search-match highlighting — the shared rich-text path of the component
//! vocabulary (ADR-020).
//!
//! Match geometry comes exclusively from the shared
//! `taskmanager_core::core::text::match_ranges_ascii_ci`; this module only
//! turns those byte ranges into segments and maps them onto iced [`Span`]s.
//! Iced 0.14's plain `text` widget cannot restyle substrings, so highlighted
//! cells render as one `Rich` span run: a single widget with the same fixed
//! width and wrapping behavior as the plain `text` cell it replaces.

use iced::widget::text::{Rich, Span};
use iced::{Element, Length};
use taskmanager_core::core::text::match_ranges_ascii_ci;

use crate::app::Message;

/// Split `text` into alternating non-matching/matching segments for `query`.
///
/// The query is trimmed before matching; an empty query and every no-match
/// case yield the whole text as one non-matching segment. Byte offsets come
/// from the shared matcher, which guarantees its ranges never split a UTF-8
/// code point; the slicing re-checks with `get` so a matcher regression can
/// never panic on a non-boundary range.
#[must_use]
pub fn highlight_segments(text: &str, query: &str) -> Vec<(String, bool)> {
    let ranges = match_ranges_ascii_ci(text, query.trim());
    if ranges.is_empty() {
        return vec![(text.to_string(), false)];
    }

    let mut segments = Vec::new();
    let mut cursor = 0;
    for range in ranges {
        let end = range.end;
        if range.start > cursor {
            segments.push((text[cursor..range.start].to_string(), false));
        }
        match text.get(range) {
            Some(matched) => segments.push((matched.to_string(), true)),
            None => {
                segments.push((text[cursor..].to_string(), false));
                return segments;
            }
        }
        cursor = end;
    }
    if cursor < text.len() {
        segments.push((text[cursor..].to_string(), false));
    }
    segments
}

/// Render one table cell. While search is inactive (or the query empty) this
/// is the plain `text` cell, unchanged; with an active search the matches
/// render in the theme accent color. Both paths keep the caller's `width`,
/// so wrapping and truncation behavior never changes with highlighting.
#[must_use]
pub fn cell(
    theme_snapshot: &taskmanager_theme::Theme,
    text: &str,
    query: &str,
    search_active: bool,
    width: Length,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    if !search_active || query.trim().is_empty() {
        return iced::widget::text(text.to_owned()).width(width).into();
    }

    let palette = theme_snapshot.palette();
    let accent = crate::theme_binding::color(palette.accent);
    let foreground = crate::theme_binding::color(palette.fg);

    let spans: Vec<Span<'static, ()>> = highlight_segments(text, query)
        .into_iter()
        .map(|(segment, matched)| {
            Span::new(segment).color(if matched { accent } else { foreground })
        })
        .collect();

    Rich::with_spans(spans).width(width).into()
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/highlight_tests.rs"]
mod tests;
