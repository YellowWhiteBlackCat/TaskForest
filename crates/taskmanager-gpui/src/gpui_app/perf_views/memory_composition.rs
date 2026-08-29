//! Memory composition proportion bar, legend, and swap annotation.

#[cfg(any(test, feature = "test-support"))]
use gpui::InteractiveElement;
use gpui::{Div, ParentElement, Styled, div, px, relative};
use taskmanager_ui_contract::IconId;

use crate::gpui_app::elements;
use crate::gpui_app::formatting::{self, DisplayUnits, UnitKind};
use taskmanager_application::i18n;
use taskmanager_core::core::metrics::MemoryMetrics;
use taskmanager_theme::tokens;
use taskmanager_theme::{Color, Theme, with_alpha};

mod stats;
use stats::{composition_labels, overview_stats, segment_shares, summary_tiles, swap_bar_stats};

type CompositionSegment = (Color, f32, String, u64);

fn memory_segments(memory: &MemoryMetrics, theme: &Theme) -> Vec<CompositionSegment> {
    // The breakdown math is shared (`taskmanager_shell::memory`) so every
    // frontend agrees on which segments exist; the data layer folds the
    // rendered width fraction, this wrapper only attaches the gpui theme
    // color.
    segment_shares(memory)
        .into_iter()
        .map(|seg| {
            (
                segment_color(seg.kind, theme),
                seg.share,
                seg.label.to_string(),
                seg.bytes,
            )
        })
        .collect()
}

/// Map a shared semantic segment kind onto the composition-bar theme color.
fn segment_color(kind: taskmanager_shell::memory::MemSegmentKind, theme: &Theme) -> Color {
    match kind {
        taskmanager_shell::memory::MemSegmentKind::Active
        | taskmanager_shell::memory::MemSegmentKind::InUse => theme.memory,
        taskmanager_shell::memory::MemSegmentKind::Inactive => with_alpha(theme.accent, 0.55),
        taskmanager_shell::memory::MemSegmentKind::Cache => with_alpha(theme.disk, 0.85),
        // Reclaimable like the page cache, but rendered as its own dimmer
        // tint so the ARC legend entry never blurs into "Cache + Buffers".
        taskmanager_shell::memory::MemSegmentKind::ZfsArc => with_alpha(theme.disk, 0.55),
        taskmanager_shell::memory::MemSegmentKind::Free
        | taskmanager_shell::memory::MemSegmentKind::Available => with_alpha(theme.fg_dim, 0.30),
        taskmanager_shell::memory::MemSegmentKind::Other => theme.shade,
    }
}

fn stacked_bar(theme: &Theme, segments: &[(Color, f32)], label: &str, height: f32) -> Div {
    let label = label.to_string();
    let share_sum: f32 = segments.iter().map(|(_, share)| *share).sum();
    let mut bar = div()
        .w_full()
        .h(px(height))
        .flex()
        .flex_row()
        .rounded(tokens::small_radius(theme))
        .overflow_hidden()
        .bg(theme.shade);
    if share_sum <= 1e-6 {
        return bar.items_center().justify_center().child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(label),
        );
    }
    for (color, share) in segments {
        let fraction = (*share / share_sum).clamp(0.0, 1.0);
        if fraction > 0.0 {
            bar = bar.child(
                div()
                    .flex_basis(relative(fraction))
                    .flex_shrink_0()
                    .h_full()
                    .bg(*color),
            );
        }
    }
    bar
}

fn composition_legend(
    theme: &Theme,
    segments: &[CompositionSegment],
    total_bytes: u64,
    units: DisplayUnits,
) -> Div {
    let total = formatting::bytes_to_gib(total_bytes.max(1));
    let mut column = div().flex().flex_col().gap(tokens::SPACE_5).w_full();
    let mut shown = false;
    for (color, _, label, bytes) in segments {
        if *bytes == 0 {
            continue;
        }
        shown = true;
        let percent = (formatting::bytes_to_gib(*bytes) / total * 100.0).clamp(0.0, 100.0);
        column = column.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(tokens::SPACE_8)
                .child(
                    div()
                        .size(px(10.0))
                        .rounded(tokens::xsmall_radius(theme))
                        .bg(*color),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(tokens::FONT_12)
                        .text_color(theme.fg)
                        .child(label.clone()),
                )
                .child(
                    div()
                        .w(px(44.0))
                        .text_size(tokens::FONT_12)
                        .text_color(theme.fg_dim)
                        .child(format!("{percent:>4.0}%")),
                )
                .child(
                    div()
                        .w(px(74.0))
                        .text_size(tokens::FONT_12)
                        .text_color(theme.fg)
                        .child(units.format(*bytes, UnitKind::Memory, false)),
                ),
        );
    }
    if shown { column } else { div() }
}

fn swap_bar(theme: &Theme, memory: &MemoryMetrics, units: DisplayUnits, with_label: bool) -> Div {
    let Some(stats) = swap_bar_stats(memory, units) else {
        return div();
    };
    let segments = [
        (with_alpha(theme.network, 0.9), stats.used_share),
        (with_alpha(theme.fg_dim, 0.22), 1.0 - stats.used_share),
    ];
    let mut bar = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_5)
        .mt(tokens::SPACE_2);
    // Compact mode keeps only the bars: the prose captions are the text the
    // memory ladder omits first (the charts-keep-their-floors policy).
    if with_label {
        bar = bar.child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(stats.label),
        );
    }
    bar.child(stacked_bar(theme, &segments, "", 8.0))
}

fn metric_tile(theme: &Theme, title: &str, value: String, note: String) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_2)
        .flex_1()
        .min_w(px(0.0))
        .child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(title.to_string()),
        )
        .child(
            // Row-wrapped truncation: a bare column-child `truncate()`
            // poisons gpui's nowrap text measure cache (see the vital line).
            div().flex().flex_row().min_w(px(0.0)).child(
                elements::truncated_text(&value)
                    .text_size(tokens::FONT_16)
                    .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                    .text_color(theme.fg),
            ),
        )
        .child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(note),
        )
}

fn summary_metrics(theme: &Theme, memory: &MemoryMetrics, units: DisplayUnits) -> Div {
    let tiles = summary_tiles(memory, units);

    div()
        .flex()
        .items_center()
        .gap(tokens::SPACE_12)
        .w_full()
        .child(metric_tile(
            theme,
            i18n::t("mem.in_use"),
            tiles.used,
            tiles.used_note,
        ))
        .child(div().w(px(1.0)).h(px(34.0)).bg(theme.border))
        .child(metric_tile(
            theme,
            i18n::t("mem.available"),
            tiles.available,
            tiles.available_note,
        ))
        .child(div().w(px(1.0)).h(px(34.0)).bg(theme.border))
        .child(metric_tile(
            theme,
            i18n::t("mem.swap"),
            tiles.swap,
            tiles.swap_note,
        ))
}

/// How much of the composition detail card the memory page's vertical
/// policy admits. The card is the page's essential multi-line fact, so its
/// degradation is TEXT-first: `Compact` keeps the composition bar (and the
/// swap bar) and drops the tiles / legend list / swap prose, while both
/// headline charts keep their floors. This is the memory page's own ladder —
/// deliberately unlike the CPU page's all-or-nothing matrix (求同存异).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemoryDetailMode {
    /// Tiles + composition caption + bar + legend list + swap annotation.
    Full,
    /// Caption + composition bar (+ swap bar): the proportions, no prose.
    Compact,
}

/// Approximate rendered height of the FULL detail card (tiles row, caption,
/// bar, five legend rows, swap annotation, paddings). A fit-check constant,
/// generous by intent: underestimating would squeeze the card.
pub(crate) const MEMORY_DETAIL_FULL_MIN_HEIGHT: f32 = 380.0;
/// Approximate rendered height of the COMPACT detail card.
pub(crate) const MEMORY_DETAIL_COMPACT_MIN_HEIGHT: f32 = 110.0;

pub(super) fn composition_block(
    theme: &Theme,
    memory: &MemoryMetrics,
    units: DisplayUnits,
    mode: MemoryDetailMode,
) -> Div {
    let overview = overview_stats(memory);
    let labels = composition_labels(memory, units);
    let total_bytes = overview.total_bytes;
    let segments = memory_segments(memory, theme);
    let bar_segments: Vec<_> = segments
        .iter()
        .map(|(color, share, _, _)| (*color, *share))
        .collect();
    let mut column = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .px(tokens::SPACE_12)
        .py(tokens::SPACE_10)
        .rounded(tokens::card_radius(theme))
        .border_1()
        .border_color(theme.border)
        .bg(theme.card_surface())
        .shadow(elements::card_shadow(theme));
    if mode == MemoryDetailMode::Full {
        column = column.child(summary_metrics(theme, memory, units));
    }
    let mut caption = div()
        .flex()
        .flex_row()
        .items_baseline()
        .justify_between()
        .child(
            div()
                .flex()
                .items_center()
                .gap(tokens::SPACE_6)
                .text_size(tokens::FONT_13)
                .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                .text_color(theme.fg)
                .child(taskmanager_icons::icon(IconId::Performance).size(px(14.0)))
                .child(i18n::t("mem.composition")),
        )
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(format!(
                    "{} {}  ·  {} {}",
                    i18n::t("mem.in_use"),
                    labels.used,
                    labels.total,
                    i18n::t("mem.total"),
                )),
        );
    #[cfg(any(test, feature = "test-support"))]
    {
        let full = mode == MemoryDetailMode::Full;
        caption = caption.debug_selector(move || {
            if full {
                "tm-mem-detail-full".into()
            } else {
                "tm-mem-detail-compact".into()
            }
        });
    }
    column = column
        .child(caption)
        .child(stacked_bar(theme, &bar_segments, &labels.pair, 20.0));
    if mode == MemoryDetailMode::Full {
        column = column.child(composition_legend(theme, &segments, total_bytes, units));
    }
    if overview.has_swap {
        column = column.child(swap_bar(
            theme,
            memory,
            units,
            mode == MemoryDetailMode::Full,
        ));
    }
    #[cfg(any(test, feature = "test-support"))]
    {
        column = column.debug_selector(|| "tm-memory-overview-card".into());
    }
    column
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_perf_views_memory_composition_tests.rs"]
mod tests;
