//! Fixed dashboard controls and whole metric groups inside the root budget.

use super::{
    DashboardViewProps, SummaryDestination, format_readout, render_history_grid, summary_card,
};
use crate::gpui_app::elements;
use crate::gpui_app::graph::graph_hover;
use crate::gpui_app::root::responsive::DashboardSummaries;
use crate::gpui_app::timeline::GraphTimelineSeries;
use gpui::{Div, InteractiveElement, IntoElement, ParentElement, Styled, div, px};
use taskmanager_application::i18n;
use taskmanager_application::system_timeline::{
    SystemHistoryWindow, TimelineMetric, TimelineSelection, TimelineStatistic,
};
use taskmanager_shell::presentation::missing_value;
use taskmanager_theme::tokens;
use taskmanager_ui::theme_binding::{definite_length, font_size, font_weight, hsla};
use taskmanager_ui_contract::IconId;

fn dashboard_summaries(props: &DashboardViewProps<'_>, series: &GraphTimelineSeries) -> Div {
    let theme = props.theme;
    let mut row = div()
        .flex()
        .flex_row()
        .gap(definite_length(tokens::SPACE_8))
        .flex_shrink_0();
    match props.layout.summaries {
        DashboardSummaries::Cards => {
            for (metric, label, color, icon, destination) in [
                (
                    TimelineMetric::Cpu,
                    i18n::t("common.cpu"),
                    theme.cpu,
                    IconId::Cpu,
                    SummaryDestination::Cpu,
                ),
                (
                    TimelineMetric::Memory,
                    i18n::t("common.memory"),
                    theme.memory,
                    IconId::Memory,
                    SummaryDestination::Memory,
                ),
            ] {
                row = row.child(summary_card(
                    theme,
                    label,
                    format_readout(
                        series,
                        TimelineSelection::new(metric, TimelineStatistic::Latest),
                        "%",
                    ),
                    color,
                    icon,
                    destination,
                    props.entity.clone(),
                ));
            }
            row = row
                .child(summary_card(
                    theme,
                    i18n::t("dashboard.processes"),
                    props
                        .process_count
                        .map_or_else(missing_value, |count| count.to_string()),
                    theme.disk,
                    IconId::Process,
                    SummaryDestination::Processes,
                    props.entity.clone(),
                ))
                .child(summary_card(
                    theme,
                    i18n::t("dashboard.active_alerts"),
                    props.active_alert_count.to_string(),
                    if props.active_alert_count == 0 {
                        theme.fg
                    } else {
                        theme.danger
                    },
                    IconId::Alert,
                    SummaryDestination::Events,
                    props.entity.clone(),
                ));
        }
        DashboardSummaries::Counts => {
            row = row
                .h(px(24.0))
                .text_size(font_size(tokens::FONT_12))
                .child(format!(
                    "{} {} · {} {}",
                    i18n::t("dashboard.processes"),
                    props
                        .process_count
                        .map_or_else(missing_value, |count| count.to_string()),
                    i18n::t("dashboard.active_alerts"),
                    props.active_alert_count
                ));
        }
        DashboardSummaries::None => {}
    }
    row
}

fn dashboard_paging(props: &DashboardViewProps<'_>) -> Div {
    let count = props.layout.metric_count.max(1);
    let first = props.layout.first_metric(props.state.history_first_metric);
    let mut row = div()
        .flex()
        .flex_row()
        .gap(definite_length(tokens::SPACE_6))
        .flex_shrink_0()
        .debug_selector(|| "tm-dashboard-paging".to_owned());
    for (previous, id, key, enabled) in [
        (
            true,
            "dashboard-previous-metrics",
            "dashboard.previous_metrics",
            first > 0,
        ),
        (
            false,
            "dashboard-next-metrics",
            "dashboard.next_metrics",
            first + count < TimelineMetric::ALL.len(),
        ),
    ] {
        let entity = props.entity.clone();
        row = row.child(
            elements::Pill::new(
                id,
                i18n::t(key),
                move |_, cx| {
                    entity.update(cx, |view, cx| {
                        view.dashboard.history_first_metric = if previous {
                            first.saturating_sub(count)
                        } else {
                            (first + count).min(TimelineMetric::ALL.len().saturating_sub(count))
                        };
                        cx.notify();
                    });
                },
                |_, _, _| {},
            )
            .enabled(enabled)
            .render(props.theme),
        );
    }
    row
}

pub fn render_dashboard(props: DashboardViewProps<'_>) -> impl IntoElement {
    let series = props
        .state
        .timeline
        .series(props.history, props.state.history_window);
    let coverage_minutes = series.covered_ms as f64 / 60_000.0;
    let mut windows = div()
        .flex()
        .flex_row()
        .gap(definite_length(tokens::SPACE_4));
    for window in SystemHistoryWindow::ALL {
        let entity = props.entity.clone();
        windows = windows.child(elements::pill(
            props.theme,
            window.id(),
            &format!("{}m", window.minutes()),
            window == props.state.history_window,
            false,
            move |_, cx| {
                entity.update(cx, |view, cx| {
                    view.dashboard.history_window = window;
                    view.dashboard.history_first_metric = 0;
                    cx.notify();
                });
            },
            |_, _, _| {},
        ));
    }
    let mut root = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .w_full()
        .gap(definite_length(tokens::SPACE_8));
    if props.layout.summaries != DashboardSummaries::None {
        root = root.child(dashboard_summaries(&props, &series));
    }
    root = root
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .flex_shrink_0()
                .gap(definite_length(tokens::SPACE_6))
                .debug_selector(|| "tm-dashboard-window-controls".to_owned())
                .child(
                    div()
                        .font_weight(font_weight(tokens::FONT_WEIGHT_SEMIBOLD))
                        .child(i18n::t("dashboard.history")),
                )
                .child(windows)
                .child(
                    div()
                        .text_size(font_size(tokens::FONT_11))
                        .text_color(hsla(props.theme.fg_dim))
                        .child(
                            i18n::t("dashboard.coverage")
                                .replace("{minutes}", &format!("{coverage_minutes:.1}")),
                        ),
                ),
        )
        .child(dashboard_paging(&props));
    let mut body = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(0.0))
        .min_w(px(0.0))
        .pb(definite_length(tokens::SPACE_8))
        .debug_selector(|| "tm-dashboard-review".to_owned());
    if props.layout.metric_count == 0 {
        body = body.child(
            div()
                .debug_selector(|| "tm-dashboard-resize".to_owned())
                .child(i18n::t("dashboard.resize")),
        );
    } else {
        body = body.child(render_history_grid(
            props.theme,
            &series,
            props.state,
            props.layout,
            props.entity,
            props.hover_slot.clone(),
            props.graph_cache,
        ));
    }
    root = root.child(body);
    if let Some((pos, text)) = graph_hover(&props.hover_slot) {
        root = root.child(elements::tooltip_overlay(props.theme, &text, pos));
    }
    root
}
