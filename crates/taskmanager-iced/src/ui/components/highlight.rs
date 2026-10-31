//! Search-match highlighting — the shared rich-text path of the component
//! vocabulary (ADR-020).
//!
//! Match geometry comes exclusively from the shared
//! `taskforest_core::core::text::match_ranges_ascii_ci`; this module only
//! turns those byte ranges into segments and maps them onto iced [`Span`]s.
//! Iced 0.14's plain `text` widget cannot restyle substrings, so highlighted
//! cells render as one `Rich` span run: a single widget with the same fixed
//! width and wrapping behavior as the plain `text` cell it replaces.

use iced::{Element, Length};
use taskforest_core::core::text::match_ranges_ascii_ci;
use zhuge_ui_core::text::{SearchTextRun, project_byte_ranges};
use zhuge_ui_kit_iced::text::highlighted;

use crate::app::Message;
use taskmanager_theme::Theme;

/// Split `text` into alternating non-matching/matching segments for `query`.
///
/// The query is trimmed before matching; an empty query and every no-match
/// case yield the whole text as one non-matching segment. Byte offsets come
/// from the shared matcher, which guarantees its ranges never split a UTF-8
/// code point; the slicing re-checks with `get` so a matcher regression can
/// never panic on a non-boundary range.
#[must_use]
pub(crate) fn cell_runs(text: &str, query: &str) -> Vec<SearchTextRun> {
    project_byte_ranges(text, match_ranges_ascii_ci(text, query.trim()))
}

/// Render one table cell. While search is inactive (or the query empty) this
/// is the plain `text` cell, unchanged; with an active search the matches
/// render in the theme accent color. Both paths keep the caller's `width`,
/// so wrapping and truncation behavior never changes with highlighting.
#[must_use]
pub fn cell(
    theme_snapshot: &Theme,
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

    highlighted(
        cell_runs(text, query)
            .into_iter()
            .map(|run| (run.text, run.highlighted)),
        foreground,
        accent,
    )
    .width(width)
    .into()
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/highlight_tests.rs"]
mod tests;
