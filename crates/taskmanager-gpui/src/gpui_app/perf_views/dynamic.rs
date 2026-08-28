//! GPUI projections for dynamic Performance devices.
//!
//! Battery and fan rendering stays at the renderer edge: it consumes typed
//! power/sensor snapshots and generation-scoped telemetry history, while the
//! provider and application layers remain unaware of GPUI layout concerns.

use std::{cell::RefCell, rc::Rc};

use gpui::{Div, ElementId, IntoElement, ParentElement, Styled, div, px};
use taskmanager_telemetry_store::TelemetryStore;

use super::device_status_i18n_key;
use super::dynamic_stats::{battery_stats, fan_stats};
use super::finite_series_peak_floored;
use super::smart_status::status_footer;
use super::{ChartSpec, HeadlineSurface, PerfPageProps, perf_page, render_chart, stats_panel};
use crate::core::{PowerSupplySnapshot, SensorCenterSnapshot, SensorQuantity};
use crate::gpui_app::formatting::{GraphUnit, PerformanceSettings};
use crate::gpui_app::graph::GraphHover;
use crate::gpui_app::history_samples::{
    battery_capacity_samples, battery_power_samples, fan_rpm_samples, fan_temperature_samples,
};
use crate::gpui_app::root::responsive::{PerformanceChartInventory, PerformancePageBudget};
use crate::gpui_app::theme::{Theme, tokens};
use crate::i18n;

/// Stateless renderer inputs for one battery detail page.
pub(crate) struct BatteryViewProps<'a> {
    pub(crate) theme: &'a Theme,
    pub(crate) power_supplies: &'a PowerSupplySnapshot,
    pub(crate) telemetry: &'a TelemetryStore,
    pub(crate) index: usize,
    pub(crate) performance: PerformanceSettings,
    pub(crate) stats_scroll: gpui::ScrollHandle,
    pub(crate) hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
    pub(crate) budget: PerformancePageBudget,
}

pub(crate) fn render_battery(props: BatteryViewProps<'_>) -> Div {
    let BatteryViewProps {
        theme,
        power_supplies,
        telemetry,
        index,
        performance,
        stats_scroll,
        hover_slot,
        budget,
    } = props;
    let Some(battery) = power_supplies.batteries.get(index) else {
        return dynamic_device_empty(
            theme,
            i18n::t("common.battery"),
            i18n::t("battery.empty"),
            power_supplies.state.status,
        );
    };
    let samples = battery_capacity_samples(
        &telemetry.dynamic_history,
        &battery.id,
        battery.device_generation,
    );
    let power_samples = battery_power_samples(
        &telemetry.dynamic_history,
        &battery.id,
        battery.device_generation,
    );
    let title = if battery.model_name.is_empty() {
        if battery.display_name.is_empty() {
            format!("{} {}", i18n::t("common.battery"), index)
        } else {
            battery.display_name.clone()
        }
    } else {
        battery.model_name.clone()
    };
    let stats = battery_stats(battery);
    // Optional power channel below the charge headline (the shared secondary
    // tier): only when the typed channel holds samples and the chart
    // inventory keeps secondary charts.
    let power_graph = (!power_samples.is_empty()
        && budget.chart_inventory == PerformanceChartInventory::Full)
        .then(|| {
            render_chart(
                theme,
                ChartSpec::secondary(
                    "battery-power-graph",
                    (ElementId::from("battery-power-graph"), battery.id.clone()),
                    i18n::t("battery.power_graph").to_string(),
                    Rc::clone(&power_samples),
                    theme.accent,
                    GraphUnit::Watts,
                )
                .with_max(finite_series_peak_floored(1.0, &power_samples)),
                performance.graph,
                budget.vertical,
                hover_slot,
            )
            .into_any_element()
        });
    perf_page(PerfPageProps {
        theme,
        stats_scroll,
        title,
        subtitle: i18n::t("battery.charge_graph").to_string(),
        header_extra: None,
        headline: HeadlineSurface::Charts(vec![ChartSpec::headline(
            "main-graph",
            (ElementId::from("tm-perf-main-graph"), battery.id.clone()),
            Rc::clone(&samples),
            theme.accent,
            GraphUnit::Percent,
        )]),
        below: power_graph,
        stats: stats_panel(theme, stats),
        stats_footer: status_footer(theme, battery.device_state.status),
        hover_slot,
        graph_settings: performance.graph,
        budget,
    })
}

/// Stateless renderer inputs for one fan detail page.
pub(crate) struct FanViewProps<'a> {
    pub(crate) theme: &'a Theme,
    pub(crate) sensors: &'a SensorCenterSnapshot,
    pub(crate) telemetry: &'a TelemetryStore,
    pub(crate) index: usize,
    pub(crate) performance: PerformanceSettings,
    pub(crate) stats_scroll: gpui::ScrollHandle,
    pub(crate) hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
    pub(crate) budget: PerformancePageBudget,
}

pub(crate) fn render_fan(props: FanViewProps<'_>) -> Div {
    let FanViewProps {
        theme,
        sensors,
        telemetry,
        index,
        performance,
        stats_scroll,
        hover_slot,
        budget,
    } = props;
    let Some(fan) = sensors
        .readings
        .iter()
        .filter(|reading| reading.quantity() == &SensorQuantity::FanSpeed)
        .nth(index)
    else {
        return dynamic_device_empty(
            theme,
            i18n::t("common.fan"),
            i18n::t("fan.empty"),
            sensors.state.status,
        );
    };
    let samples = fan_rpm_samples(
        &telemetry.dynamic_history,
        fan.id(),
        fan.device_generation(),
    );
    let max = finite_series_peak_floored(1000.0, &samples);
    let temperature_reading = sensors.readings.iter().find(|reading| {
        reading.device_id() == fan.device_id() && reading.quantity() == &SensorQuantity::Temperature
    });
    let temperature_samples = temperature_reading.map_or_else(Vec::new, |reading| {
        fan_temperature_samples(
            &telemetry.dynamic_history,
            reading.id(),
            reading.device_generation(),
        )
        .to_vec()
    });
    let stats = fan_stats(sensors, fan);
    // Optional temperature channel below the RPM headline (the shared
    // secondary tier): only when the companion channel holds samples and the
    // chart inventory keeps secondary charts.
    let temperature_graph = (!temperature_samples.is_empty()
        && budget.chart_inventory == PerformanceChartInventory::Full)
        .then(|| {
            render_chart(
                theme,
                ChartSpec::secondary(
                    "fan-temperature-graph",
                    (
                        ElementId::from("fan-temperature-graph"),
                        fan.id().to_owned(),
                    ),
                    i18n::t("fan.temperature_graph").to_string(),
                    Rc::from(temperature_samples.as_slice()),
                    theme.cpu,
                    GraphUnit::Temperature,
                )
                .with_max(finite_series_peak_floored(100.0, &temperature_samples)),
                performance.graph,
                budget.vertical,
                hover_slot,
            )
            .into_any_element()
        });
    perf_page(PerfPageProps {
        theme,
        stats_scroll,
        title: format!("{} — {}", i18n::t("common.fan"), fan.label()),
        subtitle: i18n::t("fan.speed_graph").to_string(),
        header_extra: None,
        headline: HeadlineSurface::Charts(vec![
            ChartSpec::headline(
                "main-graph",
                (ElementId::from("tm-perf-main-graph"), fan.id().to_owned()),
                Rc::clone(&samples),
                theme.cpu,
                GraphUnit::Rpm,
            )
            .with_max(max),
        ]),
        below: temperature_graph,
        stats: stats_panel(theme, stats),
        stats_footer: status_footer(theme, fan.state().status),
        hover_slot,
        graph_settings: performance.graph,
        budget,
    })
}

fn dynamic_device_empty(
    theme: &Theme,
    title: &str,
    message: &str,
    status: crate::core::DeviceStatus,
) -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .max_w(px(460.0))
                .p(tokens::SPACE_16)
                .rounded(tokens::card_radius(theme))
                .border_1()
                .border_color(theme.border)
                .bg(theme.sidebar_card_bg)
                .flex()
                .flex_col()
                .gap(tokens::SPACE_8)
                .child(
                    div()
                        .text_size(tokens::FONT_20)
                        .font_weight(tokens::FONT_WEIGHT_SEMIBOLD.into())
                        .text_color(theme.fg)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(tokens::FONT_13)
                        .text_color(theme.fg_dim)
                        .child(message.to_string()),
                )
                .child(
                    div()
                        .text_size(tokens::FONT_12)
                        .text_color(theme.fg_dim)
                        .child(i18n::t(device_status_i18n_key(status))),
                ),
        )
}
