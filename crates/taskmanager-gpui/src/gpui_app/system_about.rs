//! Mission Center-compatible System Information dialog.
//!
//! This surface is deliberately a projection only: it consumes the already
//! correlated `HardwareInfo` and desktop-appearance read models. It does not
//! read `/proc`, run commands, or invent package-manager/desktop facts that the
//! active provider did not publish.

use gpui::{
    App, ClipboardItem, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use taskmanager_ui::theme_binding::absolute;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::fill;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::hsla;

use crate::gpui_app::elements;
use crate::gpui_app::root::RootView;
use taskmanager_application::i18n;
use taskmanager_core::core::{DesktopAppearance, HardwareInfo};
use taskmanager_shell::presentation::system_information::{
    SystemInformationRow, copy_all_text, groups,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui::primitives::selectable_text::SelectableText;

/// Keep the visible prefix of provider-owned values readable in the fixed
/// two-column dialog rows. The complete value remains in the copy action; this
/// projection only prevents right-aligned overflow from hiding the useful
/// beginning of a long kernel/CPU descriptor.
fn display_value(value: &str) -> String {
    const MAX_CHARS: usize = 36;
    if value.chars().count() <= MAX_CHARS {
        return value.to_owned();
    }
    let mut displayed: String = value.chars().take(MAX_CHARS - 1).collect();
    displayed.push('…');
    displayed
}

fn render_row(
    theme: &Theme,
    row: &SystemInformationRow,
    index: usize,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let value = row.value.clone();
    let display_value = display_value(&value);
    let copy_value = format!("{}: {}", i18n::t(row.label_key), value);
    div()
        .id(("system-about-row", index))
        .debug_selector(move || format!("system-about-row-{index}"))
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(theme))
        .cursor_pointer()
        .on_click(cx.listener(move |_view, _event, _window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(copy_value.clone()));
        }))
        .px(definite_length(tokens::SPACE_10))
        .py(definite_length(tokens::SPACE_8))
        .flex()
        .items_center()
        .justify_between()
        .gap(definite_length(tokens::SPACE_12))
        .child(
            elements::truncated_text(i18n::t(row.label_key))
                .flex_1()
                .min_w(px(0.0))
                .text_size(font_size(tokens::FONT_13))
                .text_color(hsla(theme.fg)),
        )
        .child(
            div()
                .debug_selector(move || format!("system-about-value-{index}"))
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_right()
                .text_size(font_size(tokens::FONT_13))
                .text_color(hsla(theme.fg_dim))
                .child(
                    SelectableText::new(
                        ("system-about-selectable-value", index),
                        display_value,
                        theme.palette(),
                    )
                    .debug_selector(format!("system-about-selectable-value-{index}")),
                ),
        )
}

pub struct SystemAboutView {
    pub actions: Div,
    pub groups: Div,
}

pub fn render_system_about(
    theme: &Theme,
    hardware: &HardwareInfo,
    appearance: DesktopAppearance,
    entity: Entity<RootView>,
    cx: &mut Context<RootView>,
) -> SystemAboutView {
    let groups = groups(hardware, appearance);
    let copy_text = copy_all_text(&groups);
    let mut content = div()
        .flex()
        .flex_col()
        .gap(definite_length(tokens::SPACE_12));
    let mut row_index = 0;
    for (group_index, group) in groups.iter().enumerate() {
        let mut rows = div().flex().flex_col();
        for row in &group.rows {
            rows = rows.child(render_row(theme, row, row_index, cx));
            row_index += 1;
        }
        content = content.child(
            div()
                .flex()
                .flex_col()
                .gap(definite_length(tokens::SPACE_4))
                .child(
                    div()
                        .debug_selector(move || {
                            format!("tm-system-about-section-title-{group_index}")
                        })
                        .pl(definite_length(tokens::SPACE_2))
                        .text_size(font_size(tokens::FONT_12))
                        .text_color(hsla(theme.fg_dim))
                        .child(i18n::t(group.title_key)),
                )
                .child(
                    div()
                        .debug_selector(move || {
                            format!("tm-system-about-section-card-{group_index}")
                        })
                        .rounded(absolute(tokens::card_radius(theme)))
                        .bg(fill(theme.sidebar_card_bg))
                        .overflow_hidden()
                        .child(rows),
                ),
        );
    }
    if groups.is_empty() {
        content = content.child(
            div()
                .text_size(font_size(tokens::FONT_13))
                .text_color(hsla(theme.fg_dim))
                .child(i18n::t("system_about.unavailable")),
        );
    }

    let copy_entity = entity;
    let copy = div()
        .debug_selector(|| "tm-system-about-copy".to_string())
        .flex_none()
        .child(elements::pill(
            theme,
            "system-about-copy-all",
            i18n::t("system_about.copy_all"),
            true,
            false,
            move |_window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                copy_entity.update(cx, |_view, cx| cx.notify());
            },
            |_hovered: &bool, _window: &mut Window, _cx: &mut App| {},
        ));
    let actions = div()
        .debug_selector(|| "tm-system-about-actions".to_string())
        .flex()
        .flex_row()
        .items_center()
        .justify_start()
        .child(copy);
    SystemAboutView {
        actions,
        groups: content,
    }
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_system_about_tests.rs"]
mod tests;
