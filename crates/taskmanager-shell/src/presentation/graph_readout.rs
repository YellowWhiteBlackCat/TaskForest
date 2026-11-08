//! The product's graph readout: which statistics a graph window reports, in
//! which order, and the words that name them.
//!
//! The reduction itself belongs to [`super::graph_summary`]. What every
//! frontend used to spell out again was the *readout*: the same three
//! statistics, in the same order, named by the same catalog words. GPUI, Iced,
//! TUI and Bevy each joined "latest / average / peak" into their own line, so
//! a new frontend (or a new statistic) had four places to keep in step.
//!
//! This module owns the list, the order, the words and the join. A frontend
//! supplies only what it alone knows: the value spelling (its own unit
//! ladder) and, for the terminal, the separator glyph.

use taskmanager_application::i18n;

use super::GraphSummary;

/// One named statistic of a graph readout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphSummaryItem {
    /// The newest finite sample of the window.
    Latest,
    /// The mean of the window's finite samples.
    Average,
    /// The peak of the window (its maximum finite sample). The catalog word
    /// is "peak" even though the statistic is the maximum.
    Maximum,
}

impl GraphSummaryItem {
    /// Every statistic in the canonical readout order.
    pub const ALL: [Self; 3] = [Self::Latest, Self::Average, Self::Maximum];

    /// The catalog word naming this statistic.
    #[must_use]
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::Latest => "common.latest",
            Self::Average => "common.avg",
            Self::Maximum => "common.peak",
        }
    }
}

/// The localized word naming one readout statistic.
#[must_use]
pub fn graph_summary_item_label(item: GraphSummaryItem) -> &'static str {
    i18n::t(item.i18n_key())
}

/// The canonical readout of one graph window: every named statistic, in the
/// canonical order.
#[must_use]
pub fn graph_summary_items(summary: &GraphSummary) -> [(GraphSummaryItem, f32); 3] {
    [
        (GraphSummaryItem::Latest, summary.latest),
        (GraphSummaryItem::Average, summary.average),
        (GraphSummaryItem::Maximum, summary.maximum),
    ]
}

/// The value of one named statistic.
#[must_use]
pub fn graph_summary_value(summary: &GraphSummary, item: GraphSummaryItem) -> f32 {
    match item {
        GraphSummaryItem::Latest => summary.latest,
        GraphSummaryItem::Average => summary.average,
        GraphSummaryItem::Maximum => summary.maximum,
    }
}

/// The product's graph readout as one line.
///
/// The optional leading label names the graph; the named statistics follow,
/// each word followed by its formatted value, joined by `separator`. The
/// separator carries its own spacing, so the terminal can join with a
/// repertoire-aware glyph while the graphical frontends use the spaced
/// middot.
#[must_use]
pub fn graph_summary_text(
    summary: &GraphSummary,
    label: Option<&str>,
    separator: &str,
    format_value: impl Fn(f32) -> String,
) -> String {
    let mut text = String::new();
    if let Some(label) = label {
        text.push_str(label);
        text.push_str(separator);
    }
    for (index, (item, value)) in graph_summary_items(summary).into_iter().enumerate() {
        if index > 0 {
            text.push_str(separator);
        }
        text.push_str(graph_summary_item_label(item));
        text.push(' ');
        text.push_str(&format_value(value));
    }
    text
}

#[cfg(test)]
#[path = "../../tests/headless/presentation/graph_readout_tests.rs"]
mod tests;
