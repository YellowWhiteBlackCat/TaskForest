//! Bounded cell text: the overflow ellipsis.
//!
//! A table cell is a bounded slot: a value wider than its column is cut at
//! the column edge. A HARD cut loses the tail without saying so, so the cell
//! ends its value with an ellipsis instead — the same graceful degradation
//! every other frontend's table shows.
//!
//! The width authority is the SHAPED paragraph, never a character count: the
//! cut point comes from the laid-out text's own cluster advances, so
//! proportional glyphs and CJK are measured, not guessed. The ellipsis itself
//! is measured the same way — once it is on screen, its own clusters give its
//! exact width, and the next pass cuts the prefix against it.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::system::Query;
use bevy::text::{ComputedTextBlock, TextSpan};
use bevy::ui::prelude::ComputedNode;
use bevy::ui::widget::Text;

/// The suffix a truncated value ends with.
pub(crate) const ELLIPSIS: &str = "...";

/// Marker on a bounded cell's text node: this text lives in a measured slot
/// and is truncated with an ellipsis when it does not fit.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct BoundedCellText;

/// One section of a cell's text: the root text or one of its spans. The
/// sections are concatenated in this order into the paragraph the engine
/// shapes, so a byte offset in the paragraph maps back to exactly one
/// section.
struct Section {
    entity: Entity,
    start: usize,
    end: usize,
}

/// Truncate the cell texts that overflow their slot.
///
/// The system runs after the layout: the cell's content box is the available
/// width and the text node's own size is the shaped width. When the shaped
/// width exceeds the slot, the value is cut at the last cluster that still
/// fits and the ellipsis is appended.
pub(crate) fn ellipsize_overflowing_cells(
    mut cells: Query<(
        Entity,
        &BoundedCellText,
        &mut Text,
        &ComputedNode,
        &ComputedTextBlock,
        &ChildOf,
        &Children,
    )>,
    parents: Query<&ComputedNode>,
    mut spans: Query<&mut TextSpan>,
) {
    for (root, _marker, mut text, computed, block, child_of, children) in &mut cells {
        let Ok(parent) = parents.get(child_of.parent()) else {
            continue;
        };
        let available = parent.content_box().width() * parent.inverse_scale_factor();
        if available <= 0.0 {
            continue;
        }
        let shaped = computed.size().x * computed.inverse_scale_factor();
        if shaped <= available {
            continue;
        }
        // The paragraph is the concatenation of the root text and its spans,
        // in that order — the same order the engine shapes them in.
        let mut sections = vec![Section {
            entity: root,
            start: 0,
            end: text.0.len(),
        }];
        let mut paragraph = text.0.clone();
        for child in children.iter() {
            let Ok(span) = spans.get_mut(*child) else {
                continue;
            };
            let start = paragraph.len();
            paragraph.push_str(&span.0);
            sections.push(Section {
                entity: *child,
                start,
                end: paragraph.len(),
            });
        }
        let Some(cut) = cut_offset(&paragraph, available, block) else {
            continue;
        };
        let pieces = truncated_pieces(&sections, &paragraph, cut);
        if pieces[0] != text.0 {
            text.0 = pieces[0].clone();
        }
        for (section, piece) in sections.iter().skip(1).zip(pieces.iter().skip(1)) {
            let Ok(mut span) = spans.get_mut(section.entity) else {
                continue;
            };
            if span.0 != *piece {
                span.0 = piece.clone();
            }
        }
    }
}

/// The section pieces of a paragraph cut at `cut`: every section keeps the
/// part inside the cut, and the ellipsis is appended to the last piece that
/// still carries text.
fn truncated_pieces(sections: &[Section], paragraph: &str, cut: usize) -> Vec<String> {
    let mut pieces: Vec<String> = sections
        .iter()
        .map(|section| {
            let start = section.start.min(cut);
            let end = section.end.min(cut);
            paragraph[start..end].to_owned()
        })
        .collect();
    let target = pieces
        .iter()
        .rposition(|piece| !piece.is_empty())
        .unwrap_or_default();
    pieces[target].push_str(ELLIPSIS);
    pieces
}

/// The byte offset at which the value must be cut so the prefix plus the
/// ellipsis fits the slot.
///
/// The width authority is the shaped paragraph: every cluster's advance comes
/// from the laid-out text. The ellipsis width is measured from the ellipsis
/// clusters themselves once they are on screen; before that, the first cut
/// reserves nothing and the next pass corrects it.
fn cut_offset(paragraph: &str, available: f32, block: &ComputedTextBlock) -> Option<usize> {
    let layout = block.buffer();
    // The layout's metrics are already in logical pixels: the scale was
    // applied while shaping.
    let boxes_width: f32 = layout.inline_boxes().iter().map(|item| item.width).sum();
    let ellipsis_start = paragraph.len().saturating_sub(ELLIPSIS.len());
    let truncated = paragraph.ends_with(ELLIPSIS);
    // The measured width of the ellipsis itself, when it is already on
    // screen; zero on the first pass.
    let mut ellipsis_width = 0.0;
    if truncated {
        for line in layout.lines() {
            for run in line.runs() {
                for cluster in run.clusters() {
                    if cluster.text_range().start >= ellipsis_start {
                        ellipsis_width += cluster.advance();
                    }
                }
            }
        }
    }
    let budget = available - ellipsis_width - boxes_width;
    if budget <= 0.0 {
        return None;
    }
    let mut used = 0.0;
    let mut cut = paragraph.len();
    let mut last_start = 0;
    let mut measured = false;
    'clusters: for line in layout.lines() {
        for run in line.runs() {
            for cluster in run.clusters() {
                let range = cluster.text_range();
                if range.start >= paragraph.len() {
                    continue;
                }
                measured = true;
                last_start = range.start;
                used += cluster.advance();
                if used > budget {
                    cut = range.start;
                    break 'clusters;
                }
            }
        }
    }
    if !measured {
        // The text is not shaped yet (the pipeline has not run for this
        // paragraph): there is no width to cut against, so the value stays
        // whole until the next pass.
        return None;
    }
    if cut >= paragraph.len() {
        // The paragraph's own advances fit, yet the shaped width does not:
        // drop the last cluster so the ellipsis still has room.
        cut = last_start;
    }
    Some(cut.min(paragraph.len()))
}
