//! Text-rendering and high-contrast settings rows (design-debt #1/#7 line split).

use gpui::{
    AnyElement, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement, Styled, div,
};
use std::collections::HashMap;

use taskmanager_ui::inputs::switch::{Switch, SwitchState};

use crate::gpui_app::elements::Pill;
use crate::gpui_app::root::{Hover, RootView};
use taskmanager_application::i18n;
use taskmanager_core::core::config::{
    TEXT_RENDERING_GRAYSCALE, TEXT_RENDERING_PLATFORM_DEFAULT, TEXT_RENDERING_SUBPIXEL,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

pub(crate) fn text_rendering_row(
    t: &Theme,
    ent: Entity<RootView>,
    text_rendering: &'static str,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> Div {
    let cur = text_rendering;
    div()
        .flex()
        .flex_col()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_4,
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_6,
                ))
                .child(text_rendering_pill(
                    TextRenderingPillProps {
                        theme: t,
                        ent: ent.clone(),
                        id: "text-rendering-default",
                        label: i18n::t("settings.text_default"),
                        token: TEXT_RENDERING_PLATFORM_DEFAULT,
                        cur,
                        hovered,
                        enabled: true,
                    },
                    cx,
                ))
                .child(text_rendering_pill(
                    TextRenderingPillProps {
                        theme: t,
                        ent: ent.clone(),
                        id: "text-rendering-subpixel",
                        label: i18n::t("settings.text_subpixel"),
                        token: TEXT_RENDERING_SUBPIXEL,
                        cur,
                        hovered,
                        enabled: false,
                    },
                    cx,
                ))
                .child(text_rendering_pill(
                    TextRenderingPillProps {
                        theme: t,
                        ent,
                        id: "text-rendering-grayscale",
                        label: i18n::t("settings.text_grayscale"),
                        token: TEXT_RENDERING_GRAYSCALE,
                        cur,
                        hovered,
                        enabled: false,
                    },
                    cx,
                )),
        )
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(
                    tokens::FONT_CAPTION,
                ))
                .text_color(taskmanager_ui::theme_binding::hsla(t.fg_dim))
                .child(i18n::t("settings.text_rendering_unavailable")),
        )
}
struct TextRenderingPillProps<'a> {
    theme: &'a Theme,
    ent: Entity<RootView>,
    id: &'static str,
    label: &'static str,
    token: &'static str,
    cur: &'static str,
    hovered: Option<&'a Hover>,
    enabled: bool,
}
fn text_rendering_pill(
    props: TextRenderingPillProps<'_>,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let TextRenderingPillProps {
        theme: t,
        ent,
        id,
        label,
        token,
        cur,
        hovered,
        enabled,
    } = props;
    // The wrapper registers a debug selector (same test-aid contract as
    // `group`'s titles) so headless multi-window tests can click this pill by
    // id; it shrink-wraps the pill, so the pill keeps its own hover/focus/click
    // handlers and the visual tree is unchanged.
    let on_click = move |_win: &mut gpui::Window, cx: &mut gpui::App| {
        ent.update(cx, |v, cx| {
            v.set_text_rendering(token, cx);
        });
    };
    let on_hover = cx.listener(move |v, is_hov: &bool, _win, cx| {
        v.set_hover(
            if *is_hov {
                Some(Hover::Static(id))
            } else {
                None
            },
            cx,
        );
    });
    div()
        .debug_selector(move || id.to_string())
        .child(
            Pill::new(id, label, on_click, on_hover)
                .active(cur == token)
                .hovered(hovered == Some(&Hover::Static(id)))
                .enabled(enabled)
                .render(t),
        )
        .into_any_element()
}
pub(crate) fn hc_row(
    t: &Theme,
    ent: Entity<RootView>,
    switches: &HashMap<&'static str, Entity<SwitchState>>,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    let on = t.hc;
    let state = switches["hc-switch"].clone();
    state.update(cx, |state, cx| state.set_on(on, cx));
    let entity = ent.clone();
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_13))
                .text_color(taskmanager_ui::theme_binding::hsla(t.fg))
                .child(i18n::t("settings.high_contrast")),
        )
        .child(
            Switch::new(state, t.palette()).on_change(move |on, _win, cx| {
                entity.update(cx, |v, cx| {
                    v.set_high_contrast(on, cx);
                });
            }),
        )
}
