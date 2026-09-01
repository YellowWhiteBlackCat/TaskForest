//! Column-visibility picker for the process table (design-debt #1/#7 line split).

use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div,
};
use std::collections::HashSet;

use crate::gpui_app::elements;
use crate::gpui_app::processes_view::rows::{columns, header_label, is_hideable};
use crate::gpui_app::root::{Hover, RootView};
use taskmanager_application::i18n;
use taskmanager_shell::SortCol;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

pub fn columns_dropdown(
    theme: &Theme,
    hovered: Option<&Hover>,
    hidden_cols: &HashSet<SortCol>,
    swap_auto_hidden: bool,
    cx: &mut Context<RootView>,
) -> impl IntoElement {
    use taskmanager_ui::overlays::dropdown_menu::DropdownMenu;
    use taskmanager_ui::overlays::popup::{MenuEntry, MenuItem, PopupMenuState};
    let label = i18n::t("proc.choose_columns");
    let background = if hovered == Some(&Hover::Static(label)) {
        theme.hover_bg()
    } else {
        theme.sidebar_card_bg
    };
    let trigger = div()
        .debug_selector(|| "columns-trigger".into())
        .id("columns-trigger")
        .flex()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .py(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_6,
        ))
        .rounded(taskmanager_ui::theme_binding::absolute(
            tokens::control_radius(theme),
        ))
        .bg(taskmanager_ui::theme_binding::fill(background))
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_14))
        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg))
        .focusable()
        .tab_stop(true)
        .focus(elements::focus_ring(theme))
        .cursor_pointer()
        .on_hover(cx.listener(move |view, is_hovered: &bool, _, cx| {
            view.set_hover(is_hovered.then_some(Hover::Static(label)), cx);
        }))
        .child(label);
    let entity = cx.entity();
    let hidden = hidden_cols.clone();
    DropdownMenu::new(
        "columns-menu",
        trigger,
        theme.palette(),
        move |_state, cx| {
            let items = columns()
                .iter()
                .map(|&column| {
                    let entity = entity.clone();
                    let is_hidden = hidden.contains(&column);
                    let auto_hidden = column == SortCol::Swap && swap_auto_hidden;
                    MenuEntry::Item(
                        MenuItem::new(header_label(column).to_string(), move |_, cx| {
                            entity.update(cx, |view, cx| {
                                if !view.processes_state.hidden_cols.insert(column) {
                                    view.processes_state.hidden_cols.remove(&column);
                                }
                                cx.notify();
                            });
                        })
                        .checked(!is_hidden)
                        .disabled(!is_hideable(column) || auto_hidden),
                    )
                })
                .collect();
            PopupMenuState::new(items, cx)
        },
    )
}
