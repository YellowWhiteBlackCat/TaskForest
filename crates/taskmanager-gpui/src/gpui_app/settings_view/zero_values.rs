//! Apps-page zero-value presentation preference.
//!
//! This control owns only the Settings interaction. The persisted value lives
//! in `core::Config`, while the process table receives the resulting policy as
//! a render input; no provider or snapshot type needs to know about this UI
//! choice.

use std::collections::HashMap;

use gpui::{Context, Div, Entity, InteractiveElement, ParentElement, Styled, div};
use taskmanager_ui::inputs::switch::{Switch, SwitchState};

use crate::gpui_app::root::RootView;
use crate::gpui_app::theme::Theme;
use crate::gpui_app::theme::tokens;
use crate::i18n;

/// Render the Apps-page zero-value switch and its explanation.
pub(super) fn zero_values_row(
    t: &Theme,
    ent: Entity<RootView>,
    gray_zero_values: bool,
    switches: &HashMap<&'static str, Entity<SwitchState>>,
    cx: &mut Context<RootView>,
) -> Div {
    let state = switches["gray-zero-values"].clone();
    state.update(cx, |state, cx| state.set_on(gray_zero_values, cx));
    let entity = ent;
    div()
        .debug_selector(|| "gray-zero-values".to_string())
        .flex()
        .flex_col()
        .gap(tokens::SPACE_4)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(tokens::FONT_13)
                        .text_color(t.fg)
                        .child(i18n::t("settings.gray_zero_values")),
                )
                .child(
                    Switch::new(state, t.palette()).on_change(move |on, _win, cx| {
                        entity.update(cx, |view, cx| {
                            view.set_gray_zero_values(on, cx);
                        });
                    }),
                ),
        )
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(t.fg_dim)
                .child(i18n::t("settings.gray_zero_values_hint")),
        )
}
