//! Privilege helper management section inside Iced Settings.
//!
//! Provides visible status and authorization controls for optional
//! hardware capabilities (GPU engines, SMBIOS topology, RAPL power, MSR).

use iced::Length;
use iced::widget::{column, text};
use taskmanager_application::i18n::t;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use super::controls::{group_header, hint_line, setting_row};
use crate::theme::muted_text_color;
use crate::theme_binding::color;
use crate::ui::components::IcedElement;

pub(super) fn privileges_group<'a>(theme_snapshot: &'a Theme) -> IcedElement<'a> {
    let rows: Vec<IcedElement<'a>> = vec![
        hint_line(theme_snapshot, t("settings.privileges_hint")),
        privilege_status_row(
            theme_snapshot,
            t("gpu.per_engine_title"),
            t("settings.privileges_enabled"),
            true,
        ),
        privilege_status_row(
            theme_snapshot,
            t("system.memory_inventory"),
            t("settings.privileges_enabled"),
            true,
        ),
        privilege_status_row(
            theme_snapshot,
            t("cpu.package_power"),
            t("settings.privileges_authorize_hint"),
            false,
        ),
        privilege_status_row(
            theme_snapshot,
            t("cpu.msr_readouts"),
            t("settings.privileges_authorize_hint"),
            false,
        ),
    ];

    column(vec![
        group_header(theme_snapshot, t("settings.privileges")),
        column(rows).spacing(f32::from(tokens::SPACE_2)).into(),
    ])
    .spacing(f32::from(tokens::SPACE_6))
    .into()
}

fn privilege_status_row<'a>(
    theme_snapshot: &'a Theme,
    label: &'static str,
    status_label: &'static str,
    authorized: bool,
) -> IcedElement<'a> {
    let text_color = if authorized {
        color(theme_snapshot.accent)
    } else {
        muted_text_color(theme_snapshot)
    };
    setting_row(
        label,
        text(status_label)
            .size(f32::from(tokens::FONT_13))
            .color(text_color)
            .width(Length::Fill)
            .into(),
    )
}
