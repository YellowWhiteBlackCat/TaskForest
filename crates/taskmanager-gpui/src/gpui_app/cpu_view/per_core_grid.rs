//! Fixed per-core utilization matrix renderer.

use std::rc::Rc;

use gpui::{Div, InteractiveElement, ParentElement, Styled, div, px};

use crate::core::hardware::{CpuType, HardwareInfo};
use crate::gpui_app::elements;
use crate::gpui_app::graph::{GraphSettings, compute_column_count};
use crate::gpui_app::theme::{Theme, tokens};

use super::per_core::PerCoreSeries;
use super::stats::CpuLiveStats;

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
        .flex_1()
        .min_h(px(0.0))
        .gap(tokens::SPACE_10)
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
            .flex_1()
            .min_h(px(0.0))
            .gap(tokens::SPACE_4)
            .w_full()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(
                        div()
                            .text_size(tokens::FONT_11)
                            .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                            .text_color(theme.fg)
                            .child(core_type.label()),
                    )
                    .child(
                        div()
                            .text_size(tokens::FONT_10)
                            .text_color(theme.fg_dim)
                            .child(indices.len().to_string()),
                    ),
            );
        let mut rows = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .gap(tokens::SPACE_5)
            .w_full();
        for row_index in 0..row_count {
            let mut row = div()
                .flex()
                .h(px(56.0))
                .flex_shrink()
                .min_h(px(0.0))
                .gap(tokens::SPACE_5)
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
