//! Shared wrapping toolbar/action-strip surface.

use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, RenderOnce, Styled, Window,
    div,
};
use taskmanager_theme::{Length, tokens};

/// A full-width, wrapping row for page actions and controls.
///
/// The toolbar owns only the geometry. Buttons, pills, feedback, and typed
/// callbacks remain caller-owned slots.
#[derive(IntoElement)]
pub struct Toolbar {
    children: Vec<AnyElement>,
    gap: Length,
    debug_selector: Option<&'static str>,
}

impl Toolbar {
    /// Build a toolbar with the standard control rhythm.
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            gap: tokens::SPACE_8,
            debug_selector: None,
        }
    }

    /// Add one control or status slot.
    #[must_use]
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    /// Override the inter-control gap when a denser or looser strip is needed.
    #[must_use]
    pub fn gap(mut self, gap: Length) -> Self {
        self.gap = gap;
        self
    }

    /// Preserve a host-owned debug selector on the toolbar surface.
    #[must_use]
    pub fn debug_selector(mut self, selector: &'static str) -> Self {
        self.debug_selector = Some(selector);
        self
    }

    /// Render the toolbar as a concrete `Div` for callers that need to keep
    /// composing an outer page surface.
    #[must_use]
    pub fn render(self) -> gpui::Div {
        let mut toolbar = div()
            .flex()
            .flex_wrap()
            .w_full()
            .items_center()
            .gap(crate::theme_binding::definite_length(self.gap))
            .children(self.children);
        if let Some(selector) = self.debug_selector {
            toolbar = toolbar.debug_selector(move || selector.to_string());
        }
        toolbar
    }
}

impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for Toolbar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.render()
    }
}
