use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};
use taskmanager_ui_contract::IconId;

use crate::gpui_app::elements;
use crate::gpui_app::icons;
use crate::gpui_app::root::{Hover, RootView};
use crate::gpui_app::theme::{Theme, tokens};

/// Compact edit-mode affordance for the Performance sidebar. The wide sidebar
/// owns this transient toggle; row drag handles and per-device decisions are
/// mounted only while it is active, while the resulting config is persistent.
pub(super) fn edit_button(
    theme: &Theme,
    active: bool,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    div()
        .id("sidebar-edit")
        .debug_selector(|| "sidebar-edit".to_string())
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(theme))
        .cursor_pointer()
        .on_click(cx.listener(|view, _event, _window, cx| {
            view.sidebar_edit_mode = !view.sidebar_edit_mode;
            cx.notify();
        }))
        .on_hover(cx.listener(|view, is_hovered: &bool, _window, cx| {
            view.set_hover(
                if *is_hovered {
                    Some(Hover::Static("tooltip.sidebar_edit"))
                } else {
                    None
                },
                cx,
            );
        }))
        .px(tokens::SPACE_4)
        .py(tokens::SPACE_2)
        .rounded(tokens::control_radius(theme))
        .text_color(if active { theme.accent } else { theme.fg_dim })
        .child(icons::icon(IconId::Settings).size(px(14.0)))
}
