//! Compact desktop-widget projection backed by the same Dashboard read model.
//!
//! This surface intentionally has no page navigation, titlebar, or long-form
//! graph layout. It is a bounded layer-shell presentation of the existing
//! CPU, memory, process, and alert facts; the standalone RootView keeps its
//! complete desktop shell unchanged.

use gpui::{Div, InteractiveElement, ParentElement, Stateful, Styled, div, px};
use taskmanager_assets::product;
use taskmanager_ui::primitives::card_surface::CardSurface;
use taskmanager_ui_contract::IconId;

use crate::core::SystemSnapshot;
use crate::gpui_app::formatting;
use crate::gpui_app::icons;
use crate::gpui_app::theme::{Color, Theme, tokens};
use crate::i18n;

use super::readouts::cpu_summary_readout;

/// Inputs for the compact desktop widget.
pub struct DashboardWidgetProps<'a> {
    pub theme: &'a Theme,
    pub snapshot: &'a SystemSnapshot,
    pub process_count: usize,
    pub active_alert_count: usize,
}

fn metric_card(
    theme: &Theme,
    id: &'static str,
    label: String,
    value: String,
    color: Color,
    icon: IconId,
) -> Stateful<Div> {
    CardSurface::new(theme.palette())
        .background(theme.sidebar_card_bg)
        .padding(tokens::SPACE_10)
        .radius(tokens::card_radius(theme))
        .bordered(true)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(tokens::SPACE_5)
                .text_size(tokens::FONT_11)
                .text_color(theme.fg_dim)
                .child(icons::icon(icon).size(px(14.0)).text_color(color))
                .child(label),
        )
        .child(
            div()
                .mt(tokens::SPACE_6)
                .text_size(tokens::FONT_20)
                .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                .text_color(color)
                .child(value),
        )
        .render()
        .id(id)
        .debug_selector(move || id.to_owned())
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .shadow(crate::gpui_app::elements::card_shadow(theme))
}

/// Render the fixed-size desktop-widget content.
pub fn render_widget(props: DashboardWidgetProps<'_>) -> Stateful<Div> {
    let DashboardWidgetProps {
        theme,
        snapshot,
        process_count,
        active_alert_count,
    } = props;
    let alert_color = if active_alert_count == 0 {
        theme.fg
    } else {
        theme.danger
    };
    let first_row = div()
        .flex()
        .flex_row()
        .gap(tokens::SPACE_8)
        .flex_1()
        .min_h(px(0.0))
        .child(metric_card(
            theme,
            "tm-widget-cpu",
            i18n::t("common.cpu").to_string(),
            cpu_summary_readout(&snapshot.cpu),
            theme.cpu,
            IconId::Cpu,
        ))
        .child(metric_card(
            theme,
            "tm-widget-memory",
            i18n::t("common.memory").to_string(),
            snapshot
                .memory
                .used_percentage_observed()
                .map_or_else(formatting::missing_value, |value| format!("{value:.1}%")),
            theme.memory,
            IconId::Memory,
        ));
    let second_row = div()
        .flex()
        .flex_row()
        .gap(tokens::SPACE_8)
        .flex_1()
        .min_h(px(0.0))
        .child(metric_card(
            theme,
            "tm-widget-processes",
            i18n::t("dashboard.processes").to_string(),
            process_count.to_string(),
            theme.disk,
            IconId::Process,
        ))
        .child(metric_card(
            theme,
            "tm-widget-alerts",
            i18n::t("dashboard.active_alerts").to_string(),
            active_alert_count.to_string(),
            alert_color,
            IconId::Alert,
        ));

    div()
        .id("taskforest-desktop-widget")
        .debug_selector(|| "taskforest-desktop-widget".to_owned())
        .size_full()
        .p(tokens::SPACE_16)
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .h(px(24.0))
                .child(
                    div()
                        .font_weight(tokens::FONT_WEIGHT_HEADER.into())
                        .text_size(tokens::FONT_14)
                        .child(product::GPUI_NAME),
                )
                .child(
                    div()
                        .text_size(tokens::FONT_11)
                        .text_color(theme.fg_dim)
                        .child(i18n::t("dashboard.title")),
                ),
        )
        .child(first_row)
        .child(second_row)
}
