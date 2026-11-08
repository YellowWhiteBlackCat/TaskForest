//! Performance split and pinned/stacked statistics surfaces.

use super::{PERFORMANCE_STATS_MIN_WIDTH, PERFORMANCE_STATS_TRAILING_INSET};
use gpui::{Div, InteractiveElement, ParentElement, Pixels, Styled, div, px, rems};
use taskmanager_theme::tokens;
use taskmanager_theme::{GlassTier, Theme};
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::fill;
use taskmanager_ui::theme_binding::glass_fill;
use taskmanager_ui::theme_binding::hsla;
use taskmanager_ui::theme_binding::rem_length;
use taskmanager_ui_contract::presentation::vocabulary::PERFORMANCE_STATS_STACK_HEIGHT;

/// Canonical Performance page split: a shrinkable main column and a pinned,
/// non-scrolling statistics column.
pub(super) fn performance_split(theme: &Theme, left: Div, stats: Div, stats_width: f32) -> Div {
    let stats = performance_stats_surface(theme, stats, px(stats_width), true)
        .h_full()
        .debug_selector(|| "tm-perf-stats-surface".to_string());
    div()
        .flex()
        .flex_row()
        .flex_1()
        .w_full()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .size_full()
        .bg(fill(theme.window_bg))
        .child(
            left.flex_grow(1.0)
                .flex_shrink(1.0)
                .w_full()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .debug_selector(|| "tm-perf-main-surface".to_string()),
        )
        .child(stats)
}

/// Narrow-width fallback for the statistics rail. The rail remains available,
/// but it moves below the primary viewport so the graph keeps its minimum
/// readable width instead of being squeezed by two fixed columns.
pub(super) fn performance_stack(theme: &Theme, left: Div, stats: Div, stats_width: f32) -> Div {
    let stats = performance_stats_surface(theme, stats, px(stats_width), false)
        .flex_none()
        .w_full()
        .h(px(PERFORMANCE_STATS_STACK_HEIGHT))
        .max_h(px(PERFORMANCE_STATS_STACK_HEIGHT))
        .pt(definite_length(tokens::SPACE_10))
        .debug_selector(|| "tm-perf-stats-surface".to_string());
    div()
        .flex()
        .flex_col()
        .flex_1()
        .w_full()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .size_full()
        .bg(fill(theme.window_bg))
        .child(
            left.flex_1()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .debug_selector(|| "tm-perf-main-surface".to_string()),
        )
        .child(stats)
}

/// Build the one statistics surface used by both pinned and stacked modes.
pub(super) fn performance_stats_surface(
    theme: &Theme,
    stats: Div,
    width: Pixels,
    pinned: bool,
) -> Div {
    // Keep a definite readable width here. The statistics rail is deliberately
    // not a scroll owner: unavailable rows are omitted before painting and the
    // shared page budget controls the fixed-height surface.
    let mut surface = div()
        .flex()
        .flex_col()
        // `min_w` is the ELEMENT-level floor of the stats rail: the budget's
        // 236px is an input, this floor is the contract. If a caller ever hands
        // the split a width below `PERFORMANCE_STATS_MIN_WIDTH`, the width ladder
        // (Pinned → Stacked → Hidden) is the sanctioned degradation, never flex
        // squeeze.
        .flex_none()
        .flex_basis(width)
        .w(width)
        .min_w(px(PERFORMANCE_STATS_MIN_WIDTH))
        .h_full()
        .min_h(px(0.0))
        .overflow_hidden()
        .pr(PERFORMANCE_STATS_TRAILING_INSET)
        .rounded(rem_length(tokens::card_radius(theme)))
        .border(rems(GlassTier::Card.edge_width_rem()))
        .border_color(hsla(theme.glass_card().edge))
        .bg(glass_fill(theme, GlassTier::Card));
    if pinned {
        surface = surface.pl(definite_length(tokens::SPACE_16));
    }
    surface.child(stats)
}
