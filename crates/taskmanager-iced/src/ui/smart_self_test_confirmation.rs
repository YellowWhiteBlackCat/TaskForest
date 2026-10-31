//! Shared confirmation controls for the Iced SMART self-test entry points.

use iced::Element;
use iced::Length;
use iced::widget::{column, row, text};
use taskforest_core::core::smart::SmartSelfTestKind;
use taskforest_core::core::system_health::SmartSelfTestIntent;
use taskmanager_application::i18n::t;
use taskmanager_theme::{Theme, tokens};

use crate::app::{FocusTarget, Message};
use crate::focus;

pub(crate) fn confirmation<'a>(
    theme_snapshot: &'a Theme,
    pending: &SmartSelfTestIntent,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let kind = match pending.kind {
        SmartSelfTestKind::Short => t("health.kind_short"),
        SmartSelfTestKind::Extended => t("health.kind_extended"),
        SmartSelfTestKind::Conveyance => t("health.kind_conveyance"),
    };
    let summary = format!(
        "{} · {} · {}?",
        t("health.smart_self_test"),
        kind,
        pending.display_name,
    );

    column![
        text(summary)
            .size(f32::from(tokens::FONT_12))
            .width(Length::Fill),
        row![
            focus::button(
                theme_snapshot,
                FocusTarget::ConfirmSmartSelfTest,
                t("common.confirm"),
                Message::ConfirmSmartSelfTest,
                true,
            ),
            focus::button(
                theme_snapshot,
                FocusTarget::CancelSmartSelfTest,
                t("common.cancel"),
                Message::DismissOverlay,
                false,
            ),
        ]
        .spacing(8)
        .width(Length::Fill),
    ]
    .spacing(6)
    .width(Length::Fill)
    .into()
}
