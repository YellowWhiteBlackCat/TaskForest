//! Product-wide desktop interface-size chooser (Small / Standard / Large).

use gpui::{Context, Div, Entity, ParentElement, Styled, div};

use crate::gpui_app::elements::pill;
use crate::gpui_app::root::{Hover, RootView};
use taskmanager_application::i18n;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens::{self, UiSize};

pub(super) fn ui_size_row(
    theme: &Theme,
    entity: Entity<RootView>,
    active: UiSize,
    hovered: Option<&Hover>,
    cx: &mut Context<RootView>,
) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ));
    for (size, id, key) in [
        (UiSize::Small, "ui-size-small", "settings.ui_size_small"),
        (
            UiSize::Standard,
            "ui-size-standard",
            "settings.ui_size_standard",
        ),
        (UiSize::Large, "ui-size-large", "settings.ui_size_large"),
    ] {
        let click_entity = entity.clone();
        row = row.child(pill(
            theme,
            id,
            i18n::t(key),
            active == size,
            hovered == Some(&Hover::Static(id)),
            move |_window, cx| {
                click_entity.update(cx, |view, cx| {
                    view.set_ui_size(size, cx);
                });
            },
            cx.listener(move |view, is_hovered: &bool, _window, cx| {
                view.set_hover(
                    if *is_hovered {
                        Some(Hover::Static(id))
                    } else {
                        None
                    },
                    cx,
                );
            }),
        ));
    }
    row
}
