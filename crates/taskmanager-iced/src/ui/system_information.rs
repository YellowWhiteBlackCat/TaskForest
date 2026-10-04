//! Independent review of cached system information, with complete-value copies.

use super::components::IcedElement;
use super::overlays::bounded_modal_overlay;
use crate::app::{FocusTarget, Message};
use crate::focus;
use iced::Length;
use iced::widget::{column, scrollable, text};
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::system_information::SystemInformationGroup;

pub(super) fn render<'a>(
    app: &'a crate::IcedApp,
    facts: &'a [SystemInformationGroup],
) -> IcedElement<'a> {
    let mut body = column![].spacing(12).width(Length::Fill);
    if facts.is_empty() {
        body = body.push(text(t("system_about.unavailable")));
    }
    for group in facts {
        body = body.push(text(t(group.title_key)).size(18));
        for row in &group.rows {
            body = body.push(
                column![
                    text(t(row.label_key)).size(12),
                    text(row.value.clone()).size(14)
                ]
                .spacing(4)
                .width(Length::Fill),
            );
        }
    }
    let body = scrollable(body)
        .height(Length::Fill)
        .width(Length::Fill)
        .into();
    let actions = vec![focus::ghost_button(
        app.theme(),
        FocusTarget::SystemInformationCopy,
        t("system_about.copy_all"),
        Message::CopySystemInformation,
    )];
    bounded_modal_overlay(app, t("system_about.title"), body, actions)
}
