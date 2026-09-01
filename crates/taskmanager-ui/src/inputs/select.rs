//! Select: single-choice dropdown input (absorption §3.4 `Select`). The
//! trigger is a pill button showing the current option; clicking it opens
//! an anchored [`DropdownMenu`] list with one item per option. Reuses the
//! dropdown lifecycle (cached menu entity, outside-dismiss, arrow/Esc/Enter
//! navigation), so select inherits those guarantees.

use std::rc::Rc;

use gpui::{
    App, ElementId, InteractiveElement, ParentElement, SharedString, Stateful, Styled, Window, div,
    px,
};
use taskmanager_theme::Palette;

use crate::overlays::dropdown_menu::DropdownMenu;
use crate::overlays::popup::{MenuEntry, MenuItem, PopupMenuState};
use crate::styled::hover_fill;
use taskmanager_theme::tokens;

/// One selectable option.
#[derive(Clone)]
pub struct SelectOption {
    /// Stable value reported through `on_change`.
    pub value: SharedString,
    /// Label shown in the trigger and the menu.
    pub label: SharedString,
}

impl SelectOption {
    /// Build an option from a value/label pair.
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// Build a single-choice select. `value` is the current value (its option's
/// label is shown in the trigger, falling back to `placeholder`); choosing
/// an option runs `on_change(option.value, window, cx)` and closes the menu.
pub fn select(
    id: impl Into<ElementId>,
    value: Option<SharedString>,
    placeholder: impl Into<SharedString>,
    options: Vec<SelectOption>,
    palette: Palette,
    on_change: impl Fn(SharedString, &mut Window, &mut App) + 'static,
) -> DropdownMenu<Stateful<gpui::Div>> {
    let id = id.into();
    let current = options
        .iter()
        .find(|o| Some(&o.value) == value.as_ref())
        .map(|o| o.label.clone())
        .unwrap_or_else(|| placeholder.into());

    let on_change = Rc::new(on_change);
    let trigger_id = select_trigger_id(&id);
    let debug_id: String = match &trigger_id {
        ElementId::Name(name) => name.to_string(),
        other => format!("{other:?}"),
    };
    let trigger = div()
        .debug_selector(move || debug_id.clone())
        .id(trigger_id)
        .flex()
        .flex_row()
        .items_center()
        .gap(crate::theme_binding::definite_length(tokens::SPACE_6))
        .px(crate::theme_binding::definite_length(tokens::SPACE_10))
        .h(px(26.0))
        .rounded(crate::theme_binding::absolute(palette.control_radius))
        .border_1()
        .border_color(crate::theme_binding::hsla(palette.border))
        .bg(crate::theme_binding::fill(palette.surface))
        .hover(|style| style.bg(crate::theme_binding::fill(hover_fill(palette.surface))))
        .cursor_pointer()
        .text_sm()
        .text_color(crate::theme_binding::hsla(palette.fg))
        .child(current.clone())
        .child(
            div()
                .text_color(crate::theme_binding::hsla(palette.fg_muted))
                .text_size(crate::theme_binding::font_size(tokens::FONT_10))
                .child("▾"),
        );

    DropdownMenu::new(id, trigger, palette, move |_state, cx| {
        let items = options
            .iter()
            .map(|option| {
                let is_current = Some(option.value.clone()) == value;
                let on_change = on_change.clone();
                let option_value = option.value.clone();
                let option_label = option.label.clone();
                MenuEntry::Item(
                    MenuItem::new(option_label, move |window, cx| {
                        on_change(option_value.clone(), window, cx);
                    })
                    .checked(is_current),
                )
            })
            .collect();
        PopupMenuState::new(items, cx)
    })
}

/// Trigger element id derived from the select's own element id so multiple
/// selects keep distinct element state.
fn select_trigger_id(id: &ElementId) -> ElementId {
    let base = match id {
        ElementId::Name(name) => name.to_string(),
        other => format!("{other:?}"),
    };
    ElementId::Name(format!("{base}:trigger").into())
}

#[cfg(test)]
#[path = "../../tests/gui/ui_inputs_select_tests.rs"]
mod tests;
