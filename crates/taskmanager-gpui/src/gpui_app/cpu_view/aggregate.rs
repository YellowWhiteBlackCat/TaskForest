//! Dominant aggregate CPU utilization graph and its always-visible readouts.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Div, InteractiveElement, ParentElement, Styled, div, px};

use crate::gpui_app::elements;
use crate::gpui_app::graph::{GraphHover, GraphOpts, GraphSettings, graph_element_hover};
use crate::gpui_app::theme::{Theme, tokens};
use crate::i18n;

use super::CpuChartLayout;
use super::per_core::CpuAggregateSeries;
use super::stats::CpuLiveStats;

pub(super) struct AggregateGraphsProps<'a> {
    pub theme: &'a Theme,
    pub stats: &'a CpuLiveStats,
    pub series: CpuAggregateSeries,
    pub hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
    pub graph_settings: GraphSettings,
    pub layout: CpuChartLayout,
}

pub(super) fn render(props: AggregateGraphsProps<'_>) -> Div {
    let AggregateGraphsProps {
        theme,
        stats,
        series,
        hover_slot,
        graph_settings,
        layout,
    } = props;
    let samples = series.usage;
    let section = div()
        .debug_selector(|| "tm-cpu-aggregate-section".to_string())
        .flex()
        .flex_col()
        .min_w(px(0.0))
        .w_full()
        .gap(tokens::SPACE_6);
    let section = match layout {
        CpuChartLayout::AggregateWithPerCore => {
            section.flex_basis(px(190.0)).flex_shrink().min_h(px(140.0))
        }
        CpuChartLayout::AggregateOnly => section.flex_1().min_h(px(0.0)),
    };
    section.child(readouts(theme, stats)).child(
        elements::graph_card_with_state(
            theme,
            graph_element_hover(
                "cpu-headline-graph",
                "cpu-headline-graph",
                Rc::clone(&samples),
                theme.cpu.into(),
                GraphOpts {
                    max: 100.0,
                    gradient_fill: true,
                    ref_lines: true,
                    value_badge: true,
                    badge_fmt: Some(badge_usage),
                    ..GraphOpts::default()
                }
                .with_settings(graph_settings),
                |value| format!("{value:.0} %"),
                Rc::clone(hover_slot),
            ),
            &samples,
        )
        .debug_selector(|| "tm-cpu-main-utilization-graph".to_string()),
    )
}

fn readouts(theme: &Theme, stats: &CpuLiveStats) -> Div {
    let mut strip = div()
        .debug_selector(|| "tm-cpu-readouts".to_string())
        .flex()
        .flex_wrap()
        .items_baseline()
        .gap(tokens::SPACE_16)
        .child(readout(
            theme,
            i18n::t("common.utilization"),
            stats.utilization_readout.clone(),
            true,
        ));
    if let Some(frequency) = &stats.frequency_readout {
        strip = strip.child(readout(
            theme,
            i18n::t("cpu.frequency"),
            frequency.clone(),
            false,
        ));
    }
    if let Some(temperature) = &stats.temperature_readout {
        strip = strip.child(readout(
            theme,
            i18n::t("common.temperature"),
            temperature.clone(),
            false,
        ));
    }
    if let Some(power) = &stats.power_readout {
        strip = strip.child(readout(
            theme,
            i18n::t("common.power"),
            power.clone(),
            false,
        ));
    }
    strip
}

fn readout(theme: &Theme, label: &'static str, value: String, primary: bool) -> Div {
    div()
        .flex()
        .items_baseline()
        .gap(tokens::SPACE_6)
        .child(
            div()
                .text_size(tokens::FONT_13)
                .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                .text_color(if primary { theme.fg } else { theme.fg_dim })
                .child(label),
        )
        .child(
            div()
                .text_size(if primary {
                    tokens::FONT_20
                } else {
                    tokens::FONT_18
                })
                .font_weight(tokens::FONT_WEIGHT_EXTRA_BOLD.into())
                .text_color(theme.fg)
                .child(value),
        )
}

fn badge_usage(value: f32) -> String {
    format!("{value:.0} %")
}
