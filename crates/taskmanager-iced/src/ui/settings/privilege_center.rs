//! Permission rows consume the shared capability/request fold.

use iced::widget::{column, row, text};
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::privilege_center::{PrivilegeCenterInputs, PrivilegeRowState};
use taskmanager_theme::tokens;

use super::controls::{group_header, hint_line, setting_row};
use crate::app::{FocusTarget, Message};
use crate::focus;
use crate::theme::muted_text_color;
use crate::theme_binding::color;
use crate::ui::components::IcedElement;

pub(super) fn privileges_group(app: &crate::IcedApp) -> IcedElement<'_> {
    let theme = app.theme();
    let mut rows = vec![hint_line(theme, t("settings.privileges_hint"))];
    for privilege in PrivilegeCenterInputs::from_shell(&app.shell).rows() {
        let state_color = if privilege.state == PrivilegeRowState::Enabled {
            color(theme.accent)
        } else {
            muted_text_color(theme)
        };
        let mut controls = row![
            text(t(privilege.state.label_key()))
                .size(f32::from(tokens::FONT_13))
                .color(state_color)
        ]
        .spacing(f32::from(tokens::SPACE_8))
        .align_y(iced::Alignment::Center);
        if let Some(action) = privilege.action {
            controls = controls.push(focus::dynamic_button(
                theme,
                FocusTarget::SettingsChoice {
                    section: privilege.id,
                    index: 0,
                },
                t("settings.privileges_authorize").to_owned(),
                Message::AuthorizePrivilege(action),
                false,
            ));
        }
        rows.push(setting_row(t(privilege.label_key), controls.into()));
    }
    column![
        group_header(theme, t("settings.privileges")),
        column(rows).spacing(f32::from(tokens::SPACE_2))
    ]
    .spacing(f32::from(tokens::SPACE_6))
    .into()
}
