//! Read-only SMART attribute dialog rendering.

use super::smart_availability_i18n_key;
use crate::core::metrics::DiskMetrics;
use crate::gpui_app::root::prop_row;
use crate::gpui_app::theme::{Theme, tokens, with_alpha};
use crate::i18n;
use gpui::{Div, ParentElement, Styled, div, px};

pub fn render_smart_dialog(theme: &Theme, disk: &DiskMetrics) -> Div {
    let mut column = div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_6)
        .w(px(360.0))
        .child(prop_row(
            theme,
            i18n::t("common.disk"),
            disk.name.trim_start_matches("/dev/").to_string(),
        ))
        .child(prop_row(
            theme,
            i18n::t("disk.smart_status"),
            i18n::t(smart_availability_i18n_key(disk.smart_availability)).to_string(),
        ));
    if let Some(temperature) = disk.smart_temperature_c {
        let label = if disk.smart_critical_warning == Some(true) {
            "Temperature \u{26a0}"
        } else {
            i18n::t("common.temperature")
        };
        let value = match disk.smart_temp_critical_c {
            Some(critical) if critical > 0.0 => {
                format!("{temperature:.0} \u{b0}C  (critical: {critical:.0} \u{b0}C)")
            }
            _ => format!("{temperature:.0} \u{b0}C"),
        };
        column = column.child(prop_row(theme, label, value));
    }
    if let Some(critical) = disk.smart_temp_critical_c
        && disk.smart_temperature_c.is_none()
    {
        column = column.child(prop_row(
            theme,
            i18n::t("disk.critical_temp"),
            format!("{critical:.0} \u{b0}C"),
        ));
    }
    if let Some(percent) = disk.smart_percent_used {
        let value = if percent >= 100.0 {
            format!(
                "{percent:.0}% \u{26a0} {}",
                i18n::t("disk.exceeded_rated_life"),
            )
        } else {
            format!("{percent:.0}%")
        };
        column = column.child(prop_row(theme, i18n::t("disk.endurance_used"), value));
    }
    if let Some(hours) = disk.smart_power_on_hours {
        let days = hours / 24;
        column = column.child(prop_row(
            theme,
            i18n::t("disk.power_on_hours"),
            format!("{hours} h  ({:.1} yr, {days} d)", days as f64 / 365.25),
        ));
    }
    if disk.smart_critical_warning == Some(true) {
        column = column.child(
            div()
                .mt(tokens::SPACE_8)
                .px(tokens::SPACE_10)
                .py(tokens::SPACE_6)
                .rounded(tokens::small_radius(theme))
                .bg(with_alpha(theme.danger, 0.18))
                .text_size(tokens::FONT_12)
                .text_color(theme.danger)
                .child(i18n::t("disk.warning_text")),
        );
    }
    column
}
