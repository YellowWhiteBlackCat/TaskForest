//! Alert Center modal overlay for Iced.
//!
//! Renders active alert rules, threshold settings, and recent alert event history.

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_core::core::alerts::{
    AlertEvent, AlertEventKind, AlertSeverity, NotificationPolicy,
};
use taskmanager_theme::tokens;

use crate::app::{FocusTarget, Message};
use crate::focus;
use crate::theme;

pub fn alert_center_overlay<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    events: &'a [AlertEvent],
    policy: &NotificationPolicy,
    appear: f32,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    let accent = taskmanager_theme::iced::color(theme_snapshot.palette().accent);

    let quiet_status = if policy.quiet_hours.is_some() {
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

    let events_list: Element<'a, Message, iced::Theme, iced::Renderer> = if events.is_empty() {
        text(t("common.none"))
            .size(f32::from(tokens::FONT_12))
            .color(muted)
            .into()
    } else {
        let mut col = column![].spacing(6);
        for ev in events.iter().rev() {
            let sev_color = match ev.alert.severity {
                AlertSeverity::Critical => {
                    taskmanager_theme::iced::color(theme_snapshot.palette().danger)
                }
                AlertSeverity::Warning => {
                    taskmanager_theme::iced::color(theme_snapshot.palette().warning)
                }
                AlertSeverity::Info => accent,
            };
            let item_row = row![
                text(format!("[{}]", event_kind_label(ev.kind)))
                    .size(f32::from(tokens::FONT_11))
                    .color(sev_color)
                    .width(Length::Fixed(70.0)),
                text(format!("{:?}", ev.alert.metric))
                    .size(f32::from(tokens::FONT_12))
                    .width(Length::Fixed(120.0)),
                text(format!(
                    "{:.1} (thresh: {:.1})",
                    ev.alert.value, ev.alert.threshold
                ))
                .size(f32::from(tokens::FONT_12))
                .width(Length::Fixed(130.0)),
                text(&ev.alert.target)
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

fn event_kind_label(kind: AlertEventKind) -> &'static str {
    match kind {
        AlertEventKind::Activated => t("events.activated"),
        AlertEventKind::Cleared => t("events.cleared"),
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/overlays/alerts_tests.rs"]
mod tests;
