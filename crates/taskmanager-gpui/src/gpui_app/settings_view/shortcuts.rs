//! Compact, read-only shortcut legend for keyboard discoverability.

use crate::gpui_app::theme::{Theme, mono_font_with_fallback, tokens};
use crate::i18n;
use gpui::{Div, ParentElement, Styled, div};

pub(super) fn shortcut_grid(t: &Theme) -> Div {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(tokens::SPACE_6)
        .children([
            shortcut(t, "Alt+1…6", i18n::t("settings.keys_pages")),
            shortcut(t, "Ctrl+F", i18n::t("settings.keys_search")),
            shortcut(t, "Tab", i18n::t("settings.keys_focus")),
            shortcut(t, "PgUp/PgDn", i18n::t("settings.keys_select")),
            shortcut(t, "Enter", i18n::t("settings.keys_properties")),
            shortcut(t, "Delete", i18n::t("settings.keys_end_task")),
            shortcut(t, "F5", i18n::t("settings.keys_refresh")),
            shortcut(t, "F9", i18n::t("settings.keys_sidebar")),
            shortcut(t, "Esc", i18n::t("settings.keys_close")),
            shortcut(t, "Ctrl+Space", i18n::t("settings.keys_pause")),
            shortcut(t, "Ctrl", i18n::t("settings.keys_pause_hold")),
        ])
}

fn shortcut(t: &Theme, keys: &'static str, label: &'static str) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(tokens::SPACE_6)
        .px(tokens::SPACE_7)
        .py(tokens::SPACE_5)
        .rounded(tokens::small_radius(t))
        .border_1()
        .border_color(t.border)
        .bg(t.card_bg)
        .child(
            div()
                .font(mono_font_with_fallback(t))
                .text_size(tokens::FONT_11)
                .text_color(t.accent)
                .child(keys),
        )
        .child(
            div()
                .text_size(tokens::FONT_11)
                .text_color(t.fg)
                .child(label),
        )
}
