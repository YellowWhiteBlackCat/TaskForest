//! Fixed per-core utilization matrix renderer.
//!
//! Vertical contract (page policy, ADR-039): every core cell keeps the
//! `ROW_HEIGHT` floor — rows NEVER shrink. The matrix either fits under the
//! numeric check in `cpu_view` (which composes it) or hides whole, handing
//! the viewport to the aggregate chart; a squeezed half-visible row is the
//! one outcome this module refuses to paint.

use std::rc::Rc;

use gpui::{Div, InteractiveElement, ParentElement, Styled, div, px};

use crate::gpui_app::elements;
use crate::gpui_app::graph::{GraphSettings, compute_column_count};
use taskmanager_core::core::hardware::{CpuType, HardwareInfo};
use taskmanager_theme::Theme;

use super::per_core::PerCoreSeries;
use super::stats::CpuLiveStats;
use taskmanager_theme::tokens;

/// One per-core sparkline row's floor. A row may grow when the page has
/// surplus height, but never compresses below this — unsatisfiable budgets
/// hide the matrix instead (see `min_height` / the cpu_view fit check).
pub(super) const ROW_HEIGHT: f32 = 40.0;
/// A class caption ("P-cores 4") above its rows.
const SECTION_LABEL_HEIGHT: f32 = 16.0;
/// Vertical gap between row-class sections (tokens::SPACE_10).
const SECTION_GAP: f32 = 10.0;
/// Gap between a class caption and its rows, and between rows.
const INNER_GAP: f32 = 4.0;

/// The matrix's summed minimum height for this topology: the number the
/// cpu_view fit check compares against the viewport's remaining height. Rows
/// are fixed-floor, so this is exact (no text measurement involved).
pub(super) fn min_height(stats: &CpuLiveStats, hardware: &HardwareInfo) -> f32 {
    let core_count = stats.cores.len().max(1);
    let mut sections = 0.0_f32;
    let mut total = 0.0_f32;
    for core_type in [
        CpuType::Performance,
        CpuType::Efficient,
        CpuType::LowPower,
        CpuType::Unknown,
    ] {
        let indices: Vec<usize> = (0..core_count)
            .filter(|&index| {
                hardware.cpu_types.get(index).copied().unwrap_or_default() == core_type
            })
            .collect();
        if indices.is_empty() {
            continue;
        }
        let columns = compute_column_count(indices.len()).max(1);
        let row_count = indices.len().div_ceil(columns) as f32;
        total += SECTION_LABEL_HEIGHT
            + INNER_GAP
            + row_count * ROW_HEIGHT
            + (row_count - 1.0).max(0.0) * INNER_GAP;
        sections += 1.0;
    }
    total + (sections - 1.0).max(0.0) * SECTION_GAP
}

pub(super) fn render(
    theme: &Theme,
    stats: &CpuLiveStats,
    hardware: &HardwareInfo,
    series: &PerCoreSeries<'_>,
    graph_settings: GraphSettings,
) -> Div {
    let core_count = stats.cores.len().max(1);
    let mut grid = div()
        .debug_selector(|| "tm-cpu-per-core-matrix".to_string())
        .flex()
        .flex_col()
        // Grow with page surplus (sparkline rows get taller), but NEVER
        // shrink: a squeezed row reads as a broken page. The cpu_view fit
        // check hides the whole matrix when the floor cannot be met.
        .flex_grow()
        .flex_shrink_0()
        .min_h(px(0.0))
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_10,
        ))
        .w_full();
    for core_type in [
        CpuType::Performance,
        CpuType::Efficient,
        CpuType::LowPower,
        CpuType::Unknown,
    ] {
        let indices: Vec<usize> = (0..core_count)
            .filter(|&index| {
                hardware.cpu_types.get(index).copied().unwrap_or_default() == core_type
            })
            .collect();
        if indices.is_empty() {
            continue;
        }
        let color = match core_type {
            CpuType::Performance => theme.cpu,
            CpuType::Efficient => theme.network,
            CpuType::LowPower => theme.fg_dim,
            CpuType::Unknown => theme.cpu,
        };
        let columns = compute_column_count(indices.len()).max(1);
        let row_count = indices.len().div_ceil(columns);
        let mut section = div()
            .flex()
            .flex_col()
            .flex_grow()
            .flex_shrink_0()
            .min_h(px(0.0))
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_4,
            ))
            .w_full()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(
                        div()
                            .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
                            .font_weight(taskmanager_ui::theme_binding::font_weight(
                                tokens::FONT_WEIGHT_BOLD,
                            ))
                            .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                            .child(core_type.label()),
                    )
                    .child(
                        div()
                            .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_10))
                            .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                            .child(indices.len().to_string()),
                    ),
            );
        let mut rows = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .gap(px(INNER_GAP))
            .w_full();
        for row_index in 0..row_count {
            let mut row = div()
                .flex()
                .h(px(ROW_HEIGHT))
                .flex_grow()
                .flex_shrink_0()
                .gap(px(INNER_GAP))
                .w_full();
            for column_index in 0..columns {
                let position = row_index * columns + column_index;
                if position >= indices.len() {
                    row = row.child(div().flex_1().min_w(px(0.0)));
                    continue;
                }
                let core_index = indices[position];
                let samples = series
                    .usage
                    .get(core_index)
                    .cloned()
                    .unwrap_or_else(empty_samples);
                let cell = elements::mini_graph_cell(
                    theme,
                    ("tm-perf-core-graph", core_index),
                    samples,
                    color,
                    stats.cores[core_index].label(),
                    graph_settings,
                )
                .h_full()
                .min_w(px(0.0));
                #[cfg(any(test, feature = "test-support"))]
                let cell = cell.debug_selector(move || format!("tm-perf-core:{core_index}"));
                row = row.child(cell);
            }
            rows = rows.child(row);
        }
        section = section.child(rows);
        grid = grid.child(section);
    }
    grid
}

fn empty_samples() -> Rc<[f32]> {
    thread_local! {
        static EMPTY: Rc<[f32]> = Rc::from([]);
    }
    EMPTY.with(Rc::clone)
}
