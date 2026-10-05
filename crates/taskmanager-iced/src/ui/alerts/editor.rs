//! Wrapped rule cards keep whole control groups inside the bounded page body.
use crate::app::alerts::editor::RuleAdjustment;
use crate::app::alerts::{metric_label, metric_unit, severity_label};
use crate::app::{AlertsMessage, FocusTarget, Message};
use crate::{focus, theme};
use iced::widget::{column, row, text};
use iced::{Element, Length};
use taskmanager_application::{ManagedAlertRule, i18n::t};
use taskmanager_core::core::alerts::AlertMetric;
use taskmanager_theme::{Theme, tokens};

pub(super) fn card<'a>(
    theme: &'a Theme,
    index: usize,
    managed: &ManagedAlertRule,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let id = &managed.rule.id;
    let control = |slot, label: String, adjustment| {
        focus::dynamic_button(
            theme,
            FocusTarget::AlertsEdit(index, slot),
            label,
            Message::Alerts(AlertsMessage::AdjustRule {
                rule_id: id.clone(),
                adjustment,
            }),
            false,
        )
    };
    let remove = focus::button(
        theme,
        FocusTarget::AlertsEdit(index, 9),
        t("common.remove"),
        Message::Alerts(AlertsMessage::RemoveRule {
            rule_id: id.clone(),
        }),
        false,
    );
    let toggle = focus::button(
        theme,
        FocusTarget::AlertsRuleToggle(index),
        if managed.enabled {
            t("common.enabled")
        } else {
            t("common.disabled")
        },
        Message::Alerts(AlertsMessage::ToggleRule {
            rule_id: id.clone(),
        }),
        false,
    );
    let numeric = |label: &str, value: String, slot: u8, down, up| {
        row![
            text(format!("{label}: {value}")).size(f32::from(tokens::FONT_12)),
            control(slot, "−".into(), down),
            control(slot + 1, "+".into(), up)
        ]
        .spacing(f32::from(tokens::SPACE_4))
        .align_y(iced::Alignment::Center)
    };
    let fields = row![
        numeric(
            t("alerts.threshold"),
            format!(
                "{:.1}{}",
                managed.rule.threshold,
                metric_unit(managed.rule.metric)
            ),
            0,
            RuleAdjustment::Threshold(-1),
            RuleAdjustment::Threshold(1)
        ),
        numeric(
            t("alerts.duration"),
            format!("{}s", managed.rule.for_duration.as_secs()),
            2,
            RuleAdjustment::Duration(-1),
            RuleAdjustment::Duration(1)
        ),
        numeric(
            t("alerts.hysteresis"),
            format!("{:.1}", managed.rule.hysteresis),
            4,
            RuleAdjustment::Hysteresis(-1),
            RuleAdjustment::Hysteresis(1)
        ),
    ]
    .spacing(f32::from(tokens::SPACE_8))
    .wrap();
    let choices = row![
        toggle,
        control(
            6,
            metric_label(managed.rule.metric).into(),
            RuleAdjustment::Metric
        ),
        control(
            7,
            severity_label(managed.rule.severity).into(),
            RuleAdjustment::Severity
        ),
        control(
            8,
            format!(
                "{}: {}",
                t("alerts.target"),
                managed.rule.target.as_deref().unwrap_or(
                    if matches!(
                        managed.rule.metric,
                        AlertMetric::CpuUsagePercent | AlertMetric::MemoryUsagePercent
                    ) {
                        t("alerts.system_target")
                    } else {
                        t("alerts.all_disks")
                    }
                )
            ),
            RuleAdjustment::Target
        ),
        remove,
    ]
    .spacing(f32::from(tokens::SPACE_4))
    .wrap();
    iced::widget::container(
        column![
            text(id.clone()).size(f32::from(tokens::FONT_12)),
            fields,
            choices
        ]
        .spacing(f32::from(tokens::SPACE_8)),
    )
    .padding(f32::from(tokens::SPACE_8))
    .width(Length::Fill)
    .style(move |_| theme::panel_style(theme))
    .into()
}
