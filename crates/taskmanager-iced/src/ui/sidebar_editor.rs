//! Fixed editor actions around one bounded, scrollable device list.

use super::components::IcedElement;
use super::overlays::bounded_modal_overlay;
use crate::app::{FocusTarget, Message};
use crate::focus;
use iced::Length;
use iced::widget::{column, row, scrollable, text};
use taskmanager_application::i18n::t;

pub(super) fn render(app: &crate::IcedApp) -> IcedElement<'_> {
    let entries = app.sidebar_entries();
    let mut body = column![].spacing(12).width(Length::Fill);
    for (index, entry) in entries.iter().enumerate() {
        let target = |action| FocusTarget::SidebarDeviceControl { index, action };
        let mut actions = row![focus::dynamic_button(
            app.theme(),
            target(0),
            t(if entry.visible {
                "sidebar.hide_device"
            } else {
                "sidebar.show_device"
            })
            .to_owned(),
            Message::SetSidebarDeviceVisibility {
                key: entry.key.clone(),
                visible: !entry.visible
            },
            false,
        )]
        .spacing(6);
        if index > 0 {
            actions = actions.push(focus::dynamic_button(
                app.theme(),
                target(1),
                t("sidebar.move_up").to_owned(),
                Message::MoveSidebarDevice {
                    key: entry.key.clone(),
                    delta: -1,
                },
                false,
            ));
        }
        if index + 1 < entries.len() {
            actions = actions.push(focus::dynamic_button(
                app.theme(),
                target(2),
                t("sidebar.move_down").to_owned(),
                Message::MoveSidebarDevice {
                    key: entry.key.clone(),
                    delta: 1,
                },
                false,
            ));
        }
        body = body.push(
            column![text(entry.label.clone()), actions.wrap()]
                .spacing(4)
                .width(Length::Fill),
        );
    }
    bounded_modal_overlay(
        app,
        t("sidebar.edit_devices"),
        scrollable(body)
            .height(Length::Fill)
            .width(Length::Fill)
            .into(),
        vec![],
    )
}
