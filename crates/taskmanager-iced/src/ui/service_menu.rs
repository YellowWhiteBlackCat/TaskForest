//! Iced Services-row context menu.
//!
//! The menu vocabulary follows the GPUI row menu. Activation still publishes
//! the existing `RequestServiceAction` message, so selection, confirmation and
//! native service authority stay in the shared shell path.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::{ServiceAction, i18n::t};
use taskmanager_theme::{Theme, tokens};

use crate::app::{FocusTarget, Message};
use crate::{IcedApp, focus, theme};

pub(super) fn render<'a>(
    app: &IcedApp,
    theme_snapshot: &'a Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let Some(index) = app.service_menu_index() else {
        return column![].into();
    };
    let Some(service) = app.service_menu_target() else {
        return column![].into();
    };
    let actions = [
        ServiceAction::Start,
        ServiceAction::Stop,
        ServiceAction::Restart,
        ServiceAction::Enable,
        ServiceAction::Disable,
    ];
    let buttons: Vec<Element<'_, Message, iced::Theme, iced::Renderer>> = actions
        .into_iter()
        .map(|action| {
            focus::dynamic_button(
                theme_snapshot,
                FocusTarget::ServiceMenuAction { index, action },
                t(service_action_key(action)).to_owned(),
                Message::RequestServiceAction { index, action },
                matches!(
                    action,
                    ServiceAction::Stop | ServiceAction::Restart | ServiceAction::Disable
                ),
            )
        })
        .chain(std::iter::once(focus::dynamic_button(
            theme_snapshot,
            FocusTarget::ServiceMenuClose,
            t("common.cancel").to_owned(),
            Message::CloseServiceRowMenu,
            false,
        )))
        .collect();
    container(
        column![
            text(format!("{} · {}", service.name, t("common.actions")))
                .size(f32::from(tokens::FONT_12))
                .color(theme::muted_text_color(theme_snapshot)),
            row(buttons).spacing(6),
        ]
        .spacing(6)
        .padding(8),
    )
    .style(move |_| theme::panel_style(theme_snapshot))
    .width(Length::Fill)
    .into()
}

fn service_action_key(action: ServiceAction) -> &'static str {
    match action {
        ServiceAction::Start => "svc.start",
        ServiceAction::Stop => "svc.stop",
        ServiceAction::Restart => "svc.restart",
        ServiceAction::Enable => "svc.enable",
        ServiceAction::Disable => "svc.disable",
    }
}
