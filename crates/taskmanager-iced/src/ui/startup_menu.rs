//! Iced Startup-row context menu.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_theme::{Theme, tokens};

use crate::app::{FocusTarget, Message};
use crate::{IcedApp, focus, theme};

pub(super) fn render<'a>(
    app: &IcedApp,
    theme_snapshot: &'a Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let Some(index) = app.startup_menu_index() else {
        return column![].into();
    };
    let Some(entry) = app.startup_menu_entry() else {
        return column![].into();
    };
    let buttons = [true, false]
        .into_iter()
        .map(|enabled| {
            focus::dynamic_button(
                theme_snapshot,
                FocusTarget::StartupMenuAction { index, enabled },
                t(if enabled {
                    "startup.enable"
                } else {
                    "startup.disable"
                })
                .to_owned(),
                Message::RequestStartupControlFor { index, enabled },
                !enabled,
            )
        })
        .chain(std::iter::once(focus::dynamic_button(
            theme_snapshot,
            FocusTarget::StartupMenuClose,
            t("common.cancel").to_owned(),
            Message::CloseStartupRowMenu,
            false,
        )))
        .collect::<Vec<_>>();
    container(
        column![
            text(format!("{} · {}", entry.name, t("common.actions")))
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
