//! Shared process action-strip button with hover, disabled, and icon states.

use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};
use taskmanager_ui_contract::IconId;

use crate::gpui_app::elements;
use crate::gpui_app::icons;
use crate::gpui_app::root::{Hover, RootView};
use crate::gpui_app::theme::{Theme, tokens};

pub(super) struct ActionBtnProps<'a, F> {
    pub theme: &'a Theme,
    pub label: &'static str,
    pub tip: &'static str,
    pub icon: Option<IconId>,
    pub hovered: Option<&'a Hover>,
    pub enabled: bool,
    pub action: F,
}

pub(super) fn action_btn<F>(
    props: ActionBtnProps<'_, F>,
    cx: &mut Context<RootView>,
) -> impl IntoElement
where
    F: Fn(&mut RootView, &mut Context<RootView>) + 'static,
{
    let ActionBtnProps {
        theme,
        label,
        tip,
        icon,
        hovered,
        enabled,
        action,
    } = props;
    // Canonical hovered-control token — the same one the adjacent
    // columns_dropdown trigger and elements::tool_btn use — so the whole
    // action_bar breathes at one hover strength. (Was a hand-rolled
    // accent@0.12, i.e. the *selection* strength, which flashed stronger than
    // its neighbors.)
    let background = if enabled && hovered == Some(&Hover::Static(tip)) {
        theme.hover_bg()
    } else {
        theme.sidebar_card_bg
    };
    let foreground = if enabled { theme.fg } else { theme.fg_dim };
    let mut button = div()
        .id(label)
        .flex()
        .items_center()
        .gap(tokens::SPACE_6)
        .px(tokens::SPACE_12)
        .py(tokens::SPACE_6)
        .rounded(tokens::control_radius(theme))
        .bg(background)
        .text_size(tokens::FONT_13)
        .text_color(foreground);
    if let Some(icon) = icon {
        button = button.child(icons::icon(icon).size(px(14.0)));
    }
    button = button.child(label);
    if enabled {
        button
            .focusable()
            .tab_stop(true)
            .focus(elements::focus_ring(theme))
            .cursor_pointer()
            .on_click(cx.listener(move |view, _, _, cx| action(view, cx)))
            .on_hover(cx.listener(move |view, is_hovered: &bool, _, cx| {
                view.set_hover(is_hovered.then_some(Hover::Static(tip)), cx);
            }))
    } else {
        button.cursor_default().opacity(0.4)
    }
}
