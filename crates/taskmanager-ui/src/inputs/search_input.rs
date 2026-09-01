//! Search input: a text input with a search icon prefix and an Escape-cleans
//! contract (list_view search-box replacement).

use std::rc::Rc;

use crate::icons_binding::icon;
use gpui::{
    App, Entity, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div, px,
};
use taskmanager_theme::Palette;
use taskmanager_ui_contract::IconId;

use crate::OptCallback;
use crate::inputs::text_input::{TextInput, TextInputState};
use taskmanager_theme::tokens;

/// Typed search input events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchInputEvent {
    /// The query changed.
    QueryChanged,
    /// Escape pressed with a non-empty query: query cleared.
    QueryCleared,
}

/// A search field: icon prefix + text input, Escape clears when non-empty
/// (matching the app's search-box convention).
#[derive(IntoElement)]
pub struct SearchInput {
    state: Entity<TextInputState>,
    palette: Palette,
    placeholder: Option<SharedString>,
    on_change: OptCallback,
}

impl SearchInput {
    /// Build a search input over a [`TextInputState`].
    pub fn new(state: Entity<TextInputState>, palette: Palette) -> Self {
        Self {
            state,
            palette,
            placeholder: None,
            on_change: None,
        }
    }

    /// Placeholder text (default "Search…").
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Change handler fired after every edit.
    #[must_use]
    pub fn on_change(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for SearchInput {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let palette = self.palette;
        let on_change = self.on_change.clone();
        let _state = self.state.clone();

        // The input needs a placeholder default; the TextInput builder takes
        // it from the state when set. Mirror the app's "Search…" default.
        let placeholder = self.placeholder.clone().unwrap_or_else(|| "Search…".into());

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(crate::theme_binding::definite_length(tokens::SPACE_8))
            .child(
                icon(IconId::Search)
                    .size(px(14.0))
                    .text_color(crate::theme_binding::hsla(palette.fg_muted)),
            )
            .child(
                TextInput::new(self.state.clone(), palette)
                    .placeholder(placeholder)
                    .on_change(move |window, cx| {
                        if let Some(on_change) = &on_change {
                            on_change(window, cx);
                        }
                    }),
            )
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_inputs_search_input_tests.rs"]
mod tests;
