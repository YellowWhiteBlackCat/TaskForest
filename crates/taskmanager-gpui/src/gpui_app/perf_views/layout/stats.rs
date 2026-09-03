//! Static statistics rail for the shared Performance composition root.

#[cfg(any(test, feature = "test-support"))]
use gpui::InteractiveElement;
use gpui::{Div, ParentElement, Styled, div, px};
use taskmanager_application::i18n;
use taskmanager_shell::viewmodel::StatRow;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui::data::key_value_row::KeyValueRow;

use crate::gpui_app::formatting;
use crate::gpui_app::root::responsive::{
    PERFORMANCE_STATS_MAX_WIDTH, PerformanceDetailsPresentation,
};

/// The shared Performance stat panel body. A detail rail is a fixed, non-
/// scrolling surface, so a field without an accepted value is omitted before
/// layout. The permission center owns the explanation/action for fields that
/// need authorization; a dash must never consume a row and push the bottom
/// of the detail column out of the frame.
pub(crate) fn stats_panel(
    theme: &Theme,
    stats: Vec<StatRow>,
    details: PerformanceDetailsPresentation,
    content_height: f32,
) -> Div {
    const MAX_PINNED_ROWS: usize = 20;
    const MAX_STACKED_ROWS: usize = 6;
    // One row's slot includes the inter-row gap. The estimate intentionally
    // exceeds the current KeyValueRow height, so the final accepted row and
    // the optional overflow summary remain above the fixed surface edge.
    const STATS_ROW_SLOT: f32 = 36.0;
    const STATS_SURFACE_RESERVE: f32 = 64.0;
    // The stacked rail has a fixed 220px footprint. Its row cap is deliberately
    // conservative because a status/SMART footer may share that same surface.
    let hard_cap = match details {
        PerformanceDetailsPresentation::Pinned => MAX_PINNED_ROWS,
        PerformanceDetailsPresentation::Stacked => MAX_STACKED_ROWS,
        PerformanceDetailsPresentation::Hidden => 0,
    };
    let max_rows = if content_height <= 0.0 {
        hard_cap
    } else {
        let available = (content_height - STATS_SURFACE_RESERVE).max(0.0);
        let mut rows = 0_usize;
        let mut used = 0.0_f32;
        while rows < hard_cap && used + STATS_ROW_SLOT <= available {
            rows += 1;
            used += STATS_ROW_SLOT;
        }
        rows
    };
    let missing = formatting::missing_value();
    let rows: Vec<_> = stats
        .into_iter()
        .filter_map(|row| {
            let value = row.value()?.to_owned();
            (!value.trim().is_empty() && value != missing).then_some((row, value))
        })
        .collect();
    // The overflow summary consumes one more row slot; reserve it before the
    // slice is painted so the summary itself cannot become the clipped tail.
    let row_limit = if rows.len() > max_rows {
        max_rows.saturating_sub(1)
    } else {
        max_rows
    };
    let omitted = rows.len().saturating_sub(row_limit);
    let mut col = div()
        .w_full()
        .max_w(px(PERFORMANCE_STATS_MAX_WIDTH))
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_10,
        ));
    // Geometry breakpoint on the stats column root.
    #[cfg(any(test, feature = "test-support"))]
    {
        col = col.debug_selector(|| "tm-perf-stats-panel".to_string());
    }
    for (i, (row, value)) in rows.into_iter().take(row_limit).enumerate() {
        let label = row.label().to_owned();
        let key_value = if let Some((latest, average, peak)) = row.trend_parts() {
            KeyValueRow::new_multiline(label, [latest, average, peak], theme.palette())
                .value_color(theme.fg)
                .value_debug_selector(format!("tm-perf-stat-value:{i}"))
        } else {
            KeyValueRow::new(label, value, theme.palette())
                .value_color(theme.fg)
                .value_debug_selector(format!("tm-perf-stat-value:{i}"))
                .selectable_value(("perf-stat-value", i))
        };
        col = col.child(stat_row_with_selector(key_value.render(), i));
    }
    if omitted > 0 {
        col = col.child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(i18n::t("common.more_rows").replace("{count}", &omitted.to_string())),
        );
    }
    col
}

#[cfg(any(test, feature = "test-support"))]
fn stat_row_with_selector(row: Div, i: usize) -> Div {
    row.debug_selector(move || format!("tm-perf-stat:{i}"))
}

#[cfg(not(any(test, feature = "test-support")))]
fn stat_row_with_selector(row: Div, _i: usize) -> Div {
    row
}
