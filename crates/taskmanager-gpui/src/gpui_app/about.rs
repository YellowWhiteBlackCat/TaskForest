//! Independent application About projection.
//!
//! About is deliberately separate from System Information: it describes this
//! build and its distribution metadata, while `system_about` projects typed
//! host facts supplied by the correlated hardware read model. The render path
//! performs no provider or filesystem work. The repository link is submitted
//! only after an explicit button activation through RootView's typed URL-open
//! seam.

use gpui::{App, ClipboardItem, Div, Entity, ParentElement, Styled, Window, div, px};
use taskmanager_assets::product;
use taskmanager_ui::icons_binding::icon;
use taskmanager_ui::theme_binding::definite_length;
use taskmanager_ui::theme_binding::font_size;
use taskmanager_ui::theme_binding::font_weight;
use taskmanager_ui::theme_binding::hsla;

use crate::gpui_app::elements;
use crate::gpui_app::root::RootView;
use taskmanager_application::i18n;
use taskmanager_assets::product::REPOSITORY_URL;
use taskmanager_shell::presentation::about::metadata;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;
use taskmanager_ui_contract::IconId;

/// Build version compiled into this frontend binary.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn metadata_row(theme: &Theme, label: &'static str, value: impl Into<String>) -> Div {
    div()
        .flex()
        .flex_row()
        .gap(definite_length(tokens::SPACE_12))
        .items_start()
        .child(
            div()
                .w(px(94.0))
                .flex_shrink_0()
                .text_size(font_size(tokens::FONT_12))
                .text_color(hsla(theme.fg_dim))
                .child(i18n::t(label)),
        )
        .child(
            div()
                .min_w(px(0.0))
                .text_size(font_size(tokens::FONT_13))
                .text_color(hsla(theme.fg))
                .child(value.into()),
        )
}

/// Render the independent About body. `RootView` owns the modal state; this
/// module owns only the pure metadata projection and typed button callbacks.
pub fn render_about(theme: &Theme, entity: Entity<RootView>) -> Div {
    let metadata = metadata(VERSION, product::LICENSE_SPDX, product::REPOSITORY_URL);
    let copy_text = metadata.details_text();
    let open_entity = entity.clone();
    let system_entity = entity.clone();
    div()
        .w(px(430.0))
        .max_w(px(430.0))
        .flex()
        .flex_col()
        .gap(definite_length(tokens::SPACE_16))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(definite_length(tokens::SPACE_12))
                .child(
                    icon(IconId::System)
                        .size(px(38.0))
                        .text_color(hsla(theme.accent)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(definite_length(tokens::SPACE_4))
                        .min_w(px(0.0))
                        .child(
                            div()
                                .font_weight(font_weight(tokens::FONT_WEIGHT_HEADER))
                                .text_size(font_size(tokens::FONT_18))
                                .text_color(hsla(theme.fg))
                                .child(metadata.name),
                        )
                        .child(
                            div()
                                .text_size(font_size(tokens::FONT_12))
                                .text_color(hsla(theme.fg_dim))
                                .child(metadata.description),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(definite_length(tokens::SPACE_8))
                .child(metadata_row(theme, "about.version", metadata.version))
                .child(metadata_row(theme, "about.license", metadata.license))
                .child(metadata_row(theme, "about.repository", metadata.repository)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(definite_length(tokens::SPACE_8))
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
