//! Product-owned color-mode previews for the Settings modal.
//!
//! The three product palettes are the primary visual axis. Native skins only
//! contribute secondary chrome/material details, so the preview cards render
//! directly from the selected product tokens and remain recognizable while a
//! skin changes.

use gpui::{
    Context, Div, Entity, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};

use crate::core::config::{
    COLOR_SCHEME_DARK, COLOR_SCHEME_EYEFOREST, COLOR_SCHEME_LIGHT, COLOR_SCHEME_SYSTEM,
};
use crate::gpui_app::elements::pill;
use crate::gpui_app::root::{Hover, RootView};
use crate::gpui_app::theme::{LightDark, Theme, tokens};
use crate::i18n;
use taskmanager_theme::skins::tokens_for;

/// Product color-mode chooser. Light/Dark/EyeForest are the primary visual
/// axis and therefore get a real preview card; System remains a compact
/// secondary control that only follows native appearance when selected.
pub(super) fn mode_row(
    t: &Theme,
    ent: Entity<RootView>,
    color_scheme: &'static str,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_8)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .w_full()
                .gap(tokens::SPACE_8)
                .child(mode_preview_card(ModePreviewProps {
                    theme: t,
                    entity: ent.clone(),
                    id: "mode-light",
                    label: i18n::t("settings.light"),
                    mode: LightDark::Light,
                    token: COLOR_SCHEME_LIGHT,
                    selected_token: color_scheme,
                    hovered,
                }))
                .child(mode_preview_card(ModePreviewProps {
                    theme: t,
                    entity: ent.clone(),
                    id: "mode-dark",
                    label: i18n::t("settings.dark"),
                    mode: LightDark::Dark,
                    token: COLOR_SCHEME_DARK,
                    selected_token: color_scheme,
                    hovered,
                }))
                .child(mode_preview_card(ModePreviewProps {
                    theme: t,
                    entity: ent.clone(),
                    id: "mode-eyeforest",
                    label: i18n::t("settings.eyeforest"),
                    mode: LightDark::EyeForest,
                    token: COLOR_SCHEME_EYEFOREST,
                    selected_token: color_scheme,
                    hovered,
                })),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(tokens::SPACE_8)
                .child(
                    div()
                        .text_size(tokens::FONT_11)
                        .text_color(t.fg_dim)
                        .child(i18n::t("settings.system_secondary")),
                )
                .child(pill(
                    t,
                    "mode-system",
                    i18n::t("settings.system"),
                    color_scheme == COLOR_SCHEME_SYSTEM,
                    hovered == Some(&Hover::Static("mode-system")),
                    {
                        let ent = ent.clone();
                        move |_win, cx| {
                            ent.update(cx, |v, cx| {
                                v.set_color_scheme(COLOR_SCHEME_SYSTEM, cx);
                            });
                        }
                    },
                    cx.listener(move |v, is_hov: &bool, _win, cx| {
                        v.set_hover(
                            if *is_hov {
                                Some(Hover::Static("mode-system"))
                            } else {
                                None
                            },
                            cx,
                        );
                    }),
                )),
        )
}

/// A compact, keyboard-focusable product-theme preview. It intentionally uses
/// product tokens directly instead of borrowing native colors. The active
/// rail and focus ring make the card's hit target obvious without adding
/// another competing system-level color treatment.
struct ModePreviewProps<'a> {
    theme: &'a Theme,
    entity: Entity<RootView>,
    id: &'static str,
    label: &'static str,
    mode: LightDark,
    token: &'static str,
    selected_token: &'static str,
    hovered: Option<&'a Hover>,
}

fn mode_preview_card(props: ModePreviewProps<'_>) -> impl IntoElement {
    let ModePreviewProps {
        theme: t,
        entity: ent,
        id,
        label,
        mode,
        token,
        selected_token,
        hovered,
    } = props;
    let preview = tokens_for(t.skin, mode);
    let active = selected_token == token;
    let is_hov = hovered == Some(&Hover::Static(id));
    let border = if active { t.accent } else { preview.border };
    let ent_click = ent.clone();
    let ent_hover = ent.clone();
    let card = div()
        .id(id)
        // Keep the selector equal to the id so interaction probes click the
        // complete card hit area instead of an implementation-only wrapper.
        .debug_selector(move || id.to_string())
        .flex_1()
        .min_w(px(0.0))
        .w_full()
        .max_w(px(180.0))
        .p(tokens::SPACE_6)
        .flex()
        .flex_col()
        .gap(tokens::SPACE_6)
        .rounded(tokens::control_radius(t))
        .border_1()
        .border_color(border)
        .bg(if is_hov {
            preview.accent.with_alpha(0.10)
        } else {
            preview.card_bg
        })
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .focus(crate::gpui_app::elements::focus_ring(t))
        .on_click(move |_ev, _win, cx| {
            ent_click.update(cx, |v, cx| v.set_color_scheme(token, cx));
        })
        .on_hover(move |is_hov: &bool, _win, cx| {
            ent_hover.update(cx, |v, cx| {
                v.set_hover(
                    if *is_hov {
                        Some(Hover::Static(id))
                    } else {
                        None
                    },
                    cx,
                );
            });
        });
    let bars = [0.42_f32, 0.68, 0.54, 0.82, 0.61];
    card.child(
        div()
            .relative()
            .h(px(42.0))
            .w_full()
            .rounded(tokens::small_radius(t))
            .border_1()
            .border_color(preview.border)
            .bg(preview.view_bg)
            .flex()
            .flex_row()
            .gap(tokens::SPACE_4)
            .p(tokens::SPACE_4)
            .child(
                div()
                    .w(px(16.0))
                    .h_full()
                    .rounded(tokens::xsmall_radius(t))
                    .bg(preview.sidebar_bg),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_end()
                    .gap(tokens::SPACE_2)
                    .children(bars.into_iter().map(|height| {
                        div()
                            .flex_1()
                            .h(px(26.0 * height))
                            .rounded_t(tokens::xsmall_radius(t))
                            .bg(preview.accent)
                    })),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(3.0))
                    .bg(if active { t.accent } else { preview.border }),
            ),
    )
    .child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(tokens::SPACE_4)
            .min_w(px(0.0))
            .child(
                crate::gpui_app::elements::truncated_text(label)
                    .flex_1()
                    .text_size(tokens::FONT_12)
                    .font_weight(tokens::FONT_WEIGHT_SEMIBOLD.into())
                    .text_color(if active { t.fg } else { t.fg_dim }),
            )
            .child(
                div()
                    .w(px(7.0))
                    .h(px(7.0))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(preview.accent),
            ),
    )
}
