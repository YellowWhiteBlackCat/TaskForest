//! Independent application About projection.
//!
//! About is deliberately separate from System Information: it describes this
//! build and its distribution metadata, while `system_about` projects typed
//! host facts supplied by the correlated hardware read model. The render path
//! performs no provider or filesystem work. The repository link is submitted
//! only after an explicit button activation through RootView's typed URL-open
//! seam.

use gpui::{App, ClipboardItem, Div, Entity, ParentElement, Styled, Window, div, px};

use crate::gpui_app::elements;
use crate::gpui_app::root::RootView;
use taskmanager_application::i18n;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui_contract::IconId;

/// Stable repository URL shown by the About dialog and used by its explicit
/// "Open repository" action.
pub const REPOSITORY_URL: &str = "https://github.com/YellowWhiteBlackCat/TaskForest";

/// Build version compiled into this binary by Cargo.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn product_name() -> &'static str {
    i18n::t("about.name")
}

/// The exact text copied by the About dialog's Copy details action.
#[must_use]
pub fn details_text() -> String {
    format!(
        "{}\n{}: {VERSION}\n{}: {}\n{}: {REPOSITORY_URL}",
        product_name(),
        i18n::t("about.version"),
        i18n::t("about.license"),
        "Apache-2.0",
        i18n::t("about.repository"),
    )
}

fn metadata_row(theme: &Theme, label: &'static str, value: impl Into<String>) -> Div {
    div()
        .flex()
        .flex_row()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .items_start()
        .child(
            div()
                .w(px(94.0))
                .flex_shrink_0()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                .child(i18n::t(label)),
        )
        .child(
            div()
                .min_w(px(0.0))
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                .child(value.into()),
        )
}

/// Render the independent About body. `RootView` owns the modal state; this
/// module owns only the pure metadata projection and typed button callbacks.
pub fn render_about(theme: &Theme, entity: Entity<RootView>) -> Div {
    let copy_text = details_text();
    let open_entity = entity.clone();
    let system_entity = entity.clone();
    div()
        .w(px(430.0))
        .max_w(px(430.0))
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_16,
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_12,
                ))
                .child(
                    taskmanager_ui::icons_binding::icon(IconId::System)
                        .size(px(38.0))
                        .text_color(taskmanager_ui::theme_binding::hsla(theme.accent)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(taskmanager_ui::theme_binding::definite_length(
                            tokens::SPACE_4,
                        ))
                        .min_w(px(0.0))
                        .child(
                            div()
                                .font_weight(taskmanager_ui::theme_binding::font_weight(
                                    tokens::FONT_WEIGHT_HEADER,
                                ))
                                .text_size(taskmanager_ui::theme_binding::font_size(
                                    tokens::FONT_18,
                                ))
                                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
                                .child(product_name()),
                        )
                        .child(
                            div()
                                .text_size(taskmanager_ui::theme_binding::font_size(
                                    tokens::FONT_12,
                                ))
                                .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
                                .child(i18n::t("about.description")),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .child(metadata_row(theme, "about.version", VERSION))
                .child(metadata_row(theme, "about.license", "Apache-2.0"))
                .child(metadata_row(theme, "about.repository", REPOSITORY_URL)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .child(elements::pill(
                    theme,
                    "about-open-repository",
                    i18n::t("about.open_repository"),
                    false,
                    false,
                    move |_window: &mut Window, cx: &mut App| {
                        open_entity.update(cx, |view, cx| {
                            let _ = view.request_open_url(REPOSITORY_URL.to_owned(), cx);
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                ))
                .child(elements::pill(
                    theme,
                    "about-copy-details",
                    i18n::t("about.copy_details"),
                    false,
                    false,
                    move |_window: &mut Window, cx: &mut App| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                    },
                    |_, _, _| {},
                ))
                .child(elements::pill(
                    theme,
                    "about-system-information",
                    i18n::t("about.system_information"),
                    false,
                    false,
                    move |_window: &mut Window, cx: &mut App| {
                        system_entity.update(cx, |view, cx| {
                            view.show_system_about();
                            cx.notify();
                        });
                    },
                    |_, _, _| {},
                )),
        )
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_about_tests.rs"]
mod tests;
