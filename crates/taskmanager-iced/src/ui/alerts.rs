//! The Iced Alerts page: rule list, active-alert banner, empty state.
//!
//! Pure projection over shared shell state (ADR-027): the rule rows mirror
//! the frontend-local managed list ([`crate::app::alerts`]), current values
//! come from the typed snapshot accessors (an unobserved metric renders the
//! localized `None`, never a fabricated `0`), and the active-alert banner
//! reads the shell-owned `alert_active` evaluation mirror — the view never
//! evaluates alerts itself. Vocabulary reuses the shared `alerts.*` /
//! `alert.*` catalog keys the TUI suggestions overlay and the GPUI rule
//! manager already consume.

use iced::widget::{checkbox, column, container, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_core::core::alerts::AlertSeverity;
use taskmanager_ui_contract::IconId;

use crate::app::alerts::{AlertRuleRowModel, active_alert_lines, empty_state_text, rule_rows};
use crate::app::{AlertsMessage, FocusTarget, Message};
use crate::focus;
use crate::theme;
use taskmanager_theme::tokens;

/// Column widths for the rule list (layout contracts, not theme tokens).
const METRIC_CELL_WIDTH: f32 = 190.0;
const SEVERITY_CELL_WIDTH: f32 = 80.0;
const THRESHOLD_CELL_WIDTH: f32 = 90.0;
const TOGGLE_CELL_WIDTH: f32 = 150.0;

fn severity_color(
    severity: AlertSeverity,
    theme_snapshot: &taskmanager_theme::Theme,
) -> iced::Color {
    let palette = theme_snapshot.palette();
    match severity {
        AlertSeverity::Critical => taskmanager_theme::iced::color(palette.danger),
        AlertSeverity::Warning => taskmanager_theme::iced::color(palette.warning),
        AlertSeverity::Info => taskmanager_theme::iced::color(palette.accent),
    }
}

/// The page-tab pill for the nav strip. A focusable selection pill (the
/// tab-strip peer of the shared pages' pills) registered under
/// `FocusTarget::AlertsPageTab`, so the frontend-local route is Tab-reachable
/// exactly like the seven shared tabs.
pub(crate) fn page_tab_pill(
    app: &crate::IcedApp,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    focus::choice_pill_with_icon(
        theme_snapshot,
        FocusTarget::AlertsPageTab,
        IconId::Alert,
        t("alerts.manage").to_string(),
        app.alerts_page_open(),
        Message::Alerts(AlertsMessage::OpenPage),
    )
}

/// Render the Alerts page body.
pub(crate) fn render(app: &crate::IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let muted = theme::muted_text_color(theme_snapshot);

    let heading = row![
        text(t("alerts.manage")).size(f32::from(tokens::FONT_14)),
        text(t("alerts.observed_hint"))
            .size(f32::from(tokens::FONT_11))
            .color(muted),
    ]
    .spacing(f32::from(tokens::SPACE_8))
    .align_y(iced::Alignment::Center);

    let active_section = active_section(app, theme_snapshot);
    let rules_section = rules_section(app, theme_snapshot);

    let body = column![heading, active_section, rules_section]
        .spacing(f32::from(tokens::SPACE_8))
        .width(Length::Fill);

    scrollable(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn active_section<'a>(
    app: &crate::IcedApp,
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    let lines = active_alert_lines(app);
    let mut section = column![
        text(t("dashboard.active_alerts"))
            .size(f32::from(tokens::FONT_12))
            .color(muted)
    ]
    .spacing(f32::from(tokens::SPACE_4));

    if lines.is_empty() {
        section = section.push(
            text(t("common.none"))
                .size(f32::from(tokens::FONT_12))
                .color(muted),
        );
    } else {
        for line in lines {
            let color = severity_color(line.severity, theme_snapshot);
            section = section.push(
                text(line.text)
                    .size(f32::from(tokens::FONT_12))
                    .style(move |_theme| iced::widget::text::Style { color: Some(color) }),
            );
        }
    }
    container(section)
        .padding(f32::from(tokens::SPACE_8))
        .width(Length::Fill)
        .style(move |_| theme::panel_style(theme_snapshot))
        .into()
}

fn rules_section<'a>(
    app: &crate::IcedApp,
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    let rows = rule_rows(app);

    if rows.is_empty() {
        return container(
            text(empty_state_text())
                .size(f32::from(tokens::FONT_12))
                .color(muted),
        )
        .padding(f32::from(tokens::SPACE_16))
        .width(Length::Fill)
        .style(move |_| theme::panel_style(theme_snapshot))
        .into();
    }

    let header = row![
        text(t("common.name"))
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fixed(METRIC_CELL_WIDTH)),
        text(t("common.status"))
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fixed(SEVERITY_CELL_WIDTH)),
        text(t("alerts.threshold"))
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fixed(THRESHOLD_CELL_WIDTH)),
        text(t("alerts.current_value"))
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fill),
        text(t("common.enabled"))
            .size(f32::from(tokens::FONT_11))
            .color(muted)
            .width(Length::Fixed(TOGGLE_CELL_WIDTH)),
    ]
    .spacing(f32::from(tokens::SPACE_8));

    let mut list = column![header].spacing(f32::from(tokens::SPACE_4));
    for (index, row_model) in rows.into_iter().enumerate() {
        list = list.push(rule_row(theme_snapshot, index, row_model));
    }

    container(list.padding(f32::from(tokens::SPACE_8)).width(Length::Fill))
        .width(Length::Fill)
        .style(move |_| theme::panel_style(theme_snapshot))
        .into()
}

fn rule_row<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    index: usize,
    row_model: AlertRuleRowModel,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    let color = severity_color(row_model.severity, theme_snapshot);
    let enabled = row_model.enabled;
    let AlertRuleRowModel {
        rule_id,
        metric_label,
        severity_label,
        threshold_text,
        current_text,
        ..
    } = row_model;

    // The toggle cluster (checkbox + state label) is wrapped in the focusable
    // shell: Tab reaches it via `FocusTarget::AlertsRuleToggle(index)`, the
    // inner checkbox keeps the pointer path, and Enter/Space while focused
    // publishes the same ToggleRule message.
    let pointer_rule_id = rule_id.clone();
    let toggle = focus::focusable_control(
        theme_snapshot,
        FocusTarget::AlertsRuleToggle(index),
        row![
            checkbox(enabled)
                .on_toggle(move |_| {
                    Message::Alerts(AlertsMessage::ToggleRule {
                        rule_id: pointer_rule_id.clone(),
                    })
                })
                .size(f32::from(tokens::FONT_14)),
            text(if enabled {
                t("common.enabled")
            } else {
                t("common.disabled")
            })
            .size(f32::from(tokens::FONT_11))
            .color(muted),
        ]
        .spacing(f32::from(tokens::SPACE_4))
        .align_y(iced::Alignment::Center)
        .width(Length::Fixed(TOGGLE_CELL_WIDTH))
        .into(),
        Message::Alerts(AlertsMessage::ToggleRule { rule_id }),
    );

    row![
        text(metric_label)
            .size(f32::from(tokens::FONT_12))
            .width(Length::Fixed(METRIC_CELL_WIDTH)),
        text(severity_label)
            .size(f32::from(tokens::FONT_11))
            .style(move |_theme| iced::widget::text::Style { color: Some(color) })
            .width(Length::Fixed(SEVERITY_CELL_WIDTH)),
        text(threshold_text)
            .size(f32::from(tokens::FONT_12))
            .width(Length::Fixed(THRESHOLD_CELL_WIDTH)),
        text(current_text)
            .size(f32::from(tokens::FONT_12))
            .color(muted)
            .width(Length::Fill),
        toggle,
    ]
    .spacing(f32::from(tokens::SPACE_8))
    .into()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/alerts_tests.rs"]
mod tests;
