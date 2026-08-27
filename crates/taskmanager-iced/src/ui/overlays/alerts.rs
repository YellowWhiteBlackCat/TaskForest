//! Alert Center modal overlay for Iced.
//!
//! Renders active alert rules, threshold settings, and recent alert event history.

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::alerts::{AlertMetric, AlertSeverity};
use taskmanager_application::i18n::t;
use taskmanager_theme::tokens;

use crate::app::{FocusTarget, Message};
use crate::focus;
use crate::theme;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlertCenterState {
    pub quiet_hours_active: bool,
    pub events: Vec<AlertIncidentItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlertIncidentItem {
    pub id: u64,
    pub timestamp_ms: u64,
    pub metric: AlertMetric,
    pub severity: AlertSeverity,
    pub value: f32,
    pub threshold: f32,
    pub message: String,
}

pub fn alert_center_overlay<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    state: &'a AlertCenterState,
    appear: f32,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    let accent = theme::color(theme_snapshot.palette().accent);

    let quiet_status = if state.quiet_hours_active {
        text(t("common.enabled"))
            .size(f32::from(tokens::FONT_12))
            .color(accent)
    } else {
        text(t("common.disabled"))
            .size(f32::from(tokens::FONT_12))
            .color(muted)
    };

    let actions = row![
        focus::dynamic_button(
            theme_snapshot,
            FocusTarget::AlertCenterClear,
            t("common.clear").to_string(),
            Message::ClearAlertEvents,
            false,
        ),
        focus::dynamic_button(
            theme_snapshot,
            FocusTarget::AlertCenterExport,
            t("common.export").to_string(),
            Message::ExportAlertEvents,
            false,
        ),
    ]
    .spacing(8);

    let top_bar = row![quiet_status, actions]
        .spacing(12)
        .align_y(iced::Alignment::Center);

    let events_list: Element<'a, Message, iced::Theme, iced::Renderer> = if state.events.is_empty()
    {
        text(t("common.none"))
            .size(f32::from(tokens::FONT_12))
            .color(muted)
            .into()
    } else {
        let mut col = column![].spacing(6);
        for ev in &state.events {
            let sev_color = match ev.severity {
                AlertSeverity::Critical => theme::color(theme_snapshot.palette().danger),
                AlertSeverity::Warning => theme::color(theme_snapshot.palette().warning),
                AlertSeverity::Info => accent,
            };
            let item_row = row![
                text(format!("[{:?}]", ev.severity))
                    .size(f32::from(tokens::FONT_11))
                    .color(sev_color)
                    .width(Length::Fixed(70.0)),
                text(format!("{:?}", ev.metric))
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fixed(120.0)),
                text(format!("{:.1} (thresh: {:.1})", ev.value, ev.threshold))
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fixed(130.0)),
                text(&ev.message)
                    .size(f32::from(tokens::FONT_12))
                    .color(muted)
                    .width(Length::Fill),
            ]
            .spacing(8);
            col = col.push(item_row);
        }
        scrollable(col)
            .height(Length::Fixed(200.0))
            .width(Length::Fill)
            .into()
    };

    let body = column![top_bar, events_list]
        .spacing(12)
        .width(Length::Fill);

    super::modal_overlay(
        theme_snapshot,
        t("alerts.manage"),
        t("alerts.observed_hint"),
        container(body).padding(4).width(Length::Fill).into(),
        appear,
    )
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/overlays/alerts_tests.rs"]
mod tests;
