//! Iced Startup-row context menu.
//!
//! Since ICED-007 the panel is a self-owned floating surface mounted by the
//! [`crate::ui::components::Popover`] primitive on the row that opened it;
//! activation still publishes the shared startup-control messages.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::i18n::t;
use taskmanager_theme::{Theme, tokens};

use crate::app::{FocusTarget, Message};
use crate::{focus, theme};

/// The floating action panel for one open startup menu. Self-owned so the
/// row's lazy body can retain it across frames.
pub(super) fn panel(
    theme_snapshot: Theme,
    index: usize,
    entry_name: String,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let buttons = [true, false]
        .into_iter()
        .map(|enabled| {
            focus::dynamic_button_owned(
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
        .chain(std::iter::once(focus::dynamic_button_owned(
            theme_snapshot,
            FocusTarget::StartupMenuClose,
            t("common.cancel").to_owned(),
            Message::CloseStartupRowMenu,
            false,
        )))
        .collect::<Vec<_>>();
    container(
        column![
            text(format!("{entry_name} · {}", t("common.actions")))
                .size(f32::from(tokens::FONT_12))
                .color(theme::muted_text_color(&theme_snapshot)),
            row(buttons).spacing(6),
        ]
        .spacing(6)
        .padding(8),
    )
    .style(move |_| theme::panel_style(&theme_snapshot))
    .width(Length::Shrink)
    .into()
}
