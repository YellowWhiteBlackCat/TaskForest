//! Continuous durable-history preference row.

use std::collections::HashMap;

use gpui::{Context, Div, Entity, InteractiveElement, ParentElement, Styled, div};
use taskmanager_ui::inputs::switch::{Switch, SwitchState};

use crate::gpui_app::root::RootView;
use crate::gpui_app::theme::{Theme, tokens};
use crate::i18n;

pub(super) fn history_persistence_row(
    theme: &Theme,
    entity: Entity<RootView>,
    enabled: bool,
    switches: &HashMap<&'static str, Entity<SwitchState>>,
    cx: &mut Context<RootView>,
) -> Div {
    let state = switches["continuous-history"].clone();
    state.update(cx, |state, cx| state.set_on(enabled, cx));
    div()
        .debug_selector(|| "continuous-history-row".to_owned())
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(tokens::SPACE_4)
                .child(
                    div()
                        .text_size(tokens::FONT_13)
                        .text_color(theme.fg)
                        .child(i18n::t("settings.history_persistence")),
                )
                .child(
                    div()
                        .text_size(tokens::FONT_11)
                        .text_color(theme.fg_dim)
                        .child(i18n::t("settings.history_persistence_detail")),
                ),
        )
        .child(
            Switch::new(state, theme.palette()).on_change(move |enabled, _window, cx| {
                entity.update(cx, |view, cx| {
                    view.set_history_persistence(enabled, cx);
                });
            }),
        )
}
