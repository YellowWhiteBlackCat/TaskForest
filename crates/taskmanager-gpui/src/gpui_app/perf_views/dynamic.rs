//! GPUI projections for dynamic Performance devices.
//!
//! Battery and fan rendering stays at the renderer edge: it consumes typed
//! power/sensor snapshots and generation-scoped telemetry history, while the
//! provider and application layers remain unaware of GPUI layout concerns.

use std::{cell::RefCell, rc::Rc};

use gpui::{Div, ElementId, IntoElement, ParentElement, ScrollHandle, Styled, div, px};
use taskmanager_telemetry_store::TelemetryStore;

use super::device_status_i18n_key;
use super::dynamic_stats::{battery_stats, fan_stats};
use super::smart_status::status_footer;
use crate::core::{PowerSupplySnapshot, SensorCenterSnapshot, SensorQuantity};
use crate::gpui_app::formatting::{GraphUnit, PerformanceSettings};
use crate::gpui_app::graph::{GraphHover, GraphOpts};
use crate::gpui_app::history_samples::{
    battery_capacity_samples, battery_power_samples, fan_rpm_samples, fan_temperature_samples,
};
use crate::gpui_app::perf_views::layout::{
    MainColumnLayout, MainContent, MainWithStatsProps, SecondaryGraphCardProps, main_with_stats,
    secondary_graph_card,
};
use crate::gpui_app::theme::{Theme, tokens};
use crate::i18n;

/// Stateless renderer inputs for one battery detail page.
pub(crate) struct BatteryViewProps<'a> {
    pub(crate) theme: &'a Theme,
    pub(crate) power_supplies: &'a PowerSupplySnapshot,
    pub(crate) telemetry: &'a TelemetryStore,
    pub(crate) index: usize,
    pub(crate) performance: PerformanceSettings,
    pub(crate) left_scroll: ScrollHandle,
    pub(crate) stats_scroll: ScrollHandle,
    pub(crate) hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
}

pub(crate) fn render_battery(props: BatteryViewProps<'_>) -> Div {
    let BatteryViewProps {
        theme,
        power_supplies,
        telemetry,
        index,
        performance,
        left_scroll,
        stats_scroll,
        hover_slot,
    } = props;
    let graph_settings = performance.graph;
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
    let left_footer = (!power_samples.is_empty()).then(|| {
        let max = power_samples
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .fold(1.0_f32, f32::max);
        secondary_graph_card(SecondaryGraphCardProps {
            theme,
            id: ElementId::from("battery-power-graph"),
            slide_key: (ElementId::from("battery-power-graph"), battery.id.clone()).into(),
            title: i18n::t("battery.power_graph").to_string(),
            samples: Rc::clone(&power_samples),
            color: theme.accent,
            graph_opts: GraphOpts {
                max,
                ..GraphOpts::default()
            },
            graph_settings,
            graph_unit: GraphUnit::Watts,
            hover_slot,
        })
        .into_any_element()
    });
    main_with_stats(MainWithStatsProps {
        theme,
        left_scroll,
        stats_scroll,
        title,
        subtitle: i18n::t("battery.charge_graph").to_string(),
        graph_id: (ElementId::from("tm-perf-main-graph"), battery.id.clone()).into(),
        graph_samples: Rc::clone(&samples),
        graph_color: theme.accent,
        graph_opts: GraphOpts::default(),
        graph_settings,
        graph_unit: GraphUnit::Percent,
        graph_dual: None,
        main_content: MainContent::AggregateGraph,
        main_column: MainColumnLayout::Scrollable,
        graph_controls: None,
        stats,
        stats_footer: status_footer(theme, battery.device_state.status),
        left_footer,
        hover_slot,
    })
}

/// Stateless renderer inputs for one fan detail page.
pub(crate) struct FanViewProps<'a> {
    pub(crate) theme: &'a Theme,
    pub(crate) sensors: &'a SensorCenterSnapshot,
    pub(crate) telemetry: &'a TelemetryStore,
    pub(crate) index: usize,
    pub(crate) performance: PerformanceSettings,
    pub(crate) left_scroll: ScrollHandle,
    pub(crate) stats_scroll: ScrollHandle,
    pub(crate) hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
}

pub(crate) fn render_fan(props: FanViewProps<'_>) -> Div {
    let FanViewProps {
        theme,
        sensors,
        telemetry,
        index,
        performance,
        left_scroll,
        stats_scroll,
        hover_slot,
    } = props;
    let graph_settings = performance.graph;
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
    let max = samples.iter().copied().fold(1000.0_f32, f32::max);
    let temperature_reading = sensors.readings.iter().find(|reading| {
        reading.device_id() == fan.device_id() && reading.quantity() == &SensorQuantity::Temperature
    });
    let empty: std::rc::Rc<[f32]> = std::rc::Rc::from([]);
    let temperature_samples = temperature_reading.map_or_else(
        || empty.clone(),
        |reading| {
            fan_temperature_samples(
                &telemetry.dynamic_history,
                reading.id(),
                reading.device_generation(),
            )
        },
    );
    let stats = fan_stats(sensors, fan);
    let left_footer = (!temperature_samples.is_empty()).then(|| {
        let max = temperature_samples
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .fold(100.0_f32, f32::max);
        secondary_graph_card(SecondaryGraphCardProps {
            theme,
            id: ElementId::from("fan-temperature-graph"),
            slide_key: (
                ElementId::from("fan-temperature-graph"),
                fan.id().to_owned(),
            )
                .into(),
            title: i18n::t("fan.temperature_graph").to_string(),
            samples: Rc::clone(&temperature_samples),
            color: theme.cpu,
            graph_opts: GraphOpts {
                max,
                ..GraphOpts::default()
            },
            graph_settings,
            graph_unit: GraphUnit::Temperature,
            hover_slot,
        })
        .into_any_element()
    });
    main_with_stats(MainWithStatsProps {
        theme,
        left_scroll,
        stats_scroll,
        title: format!("{} — {}", i18n::t("common.fan"), fan.label()),
        subtitle: i18n::t("fan.speed_graph").to_string(),
        graph_id: (ElementId::from("tm-perf-main-graph"), fan.id().to_owned()).into(),
        graph_samples: Rc::clone(&samples),
        graph_color: theme.cpu,
        graph_opts: GraphOpts {
            max,
            ..GraphOpts::default()
        },
        graph_settings,
        graph_unit: GraphUnit::Rpm,
        graph_dual: None,
        main_content: MainContent::AggregateGraph,
        main_column: MainColumnLayout::Scrollable,
        graph_controls: None,
        stats,
        stats_footer: status_footer(theme, fan.state().status),
        left_footer,
        hover_slot,
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
