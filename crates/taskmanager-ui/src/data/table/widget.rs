//! Table element wrapper bound to the shared TableState.

use super::{TABLE_CONTEXT, TableDelegate, TableOptions, TableState};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, Edges, ElementId, Entity, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Styled, Window, div,
};
use taskmanager_theme::Palette;

/// The table element: binds the state to a key context, focus, and actions.
#[derive(IntoElement)]
pub struct Table<D: TableDelegate> {
    state: Entity<TableState<D>>,
    palette: Palette,
    options: TableOptions,
}

impl<D: TableDelegate> Table<D> {
    /// Build a table for `state` with the given color contract.
    pub fn new(state: &Entity<TableState<D>>, palette: Palette) -> Self {
        Self {
            state: state.clone(),
            palette,
            options: TableOptions::default(),
        }
    }

    /// Zebra striping (default false).
    #[must_use]
    pub fn stripe(mut self, stripe: bool) -> Self {
        self.options.stripe = stripe;
        self
    }

    /// Rounded, bordered container (default true).
    #[must_use]
    pub fn bordered(mut self, bordered: bool) -> Self {
        self.options.bordered = bordered;
        self
    }

    /// Scrollbar visibility (default both visible).
    #[must_use]
    pub fn scrollbar_visible(mut self, vertical: bool, horizontal: bool) -> Self {
        self.options.scrollbar_visible = Edges {
            right: vertical,
            bottom: horizontal,
            ..Default::default()
        };
        self
    }

    /// Uniform row height (default 28px).
    #[must_use]
    pub fn row_height(mut self, row_height: impl Into<Pixels>) -> Self {
        self.options.row_height = row_height.into();
        self
    }
}

impl<D: TableDelegate> RenderOnce for Table<D> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus_handle = self
            .state
            .read_with(cx, |state, _| state.focus_handle.clone());
        let bordered = self.options.bordered;
        let palette = self.palette;
        self.state.update(cx, |state, _| {
            state.options = self.options;
            state.palette = palette;
        });

        div()
            .id(ElementId::named_usize(
                "tm-table",
                self.state.entity_id().as_non_zero_u64().get() as usize,
            ))
            .debug_selector(|| "tm-table".into())
            .size_full()
            .key_context(TABLE_CONTEXT)
            .track_focus(&focus_handle)
            .on_action(window.listener_for(&self.state, TableState::action_select_up))
            .on_action(window.listener_for(&self.state, TableState::action_select_down))
            .on_action(window.listener_for(&self.state, TableState::action_select_prev_col))
            .on_action(window.listener_for(&self.state, TableState::action_select_next_col))
            .on_action(window.listener_for(&self.state, TableState::action_select_home))
            .on_action(window.listener_for(&self.state, TableState::action_select_end))
            .on_action(window.listener_for(&self.state, TableState::action_activate))
            .on_action(window.listener_for(&self.state, TableState::action_cancel))
            .bg(crate::theme_binding::fill(palette.surface))
            .when(bordered, |this| {
                this.rounded(crate::theme_binding::absolute(palette.panel_radius))
                    .border_1()
                    .border_color(crate::theme_binding::hsla(palette.border))
            })
            .child(self.state)
    }
}
