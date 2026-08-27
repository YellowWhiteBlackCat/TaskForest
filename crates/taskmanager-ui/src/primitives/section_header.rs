//! Shared titled-section header geometry.

use gpui::{
    App, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window,
    div, px,
};
use taskmanager_theme::{Palette, tokens};

/// A semantic section title followed by the palette hairline divider.
///
/// The title text and the body remain caller-owned. This primitive only
/// standardizes the heading geometry used by settings, help, and other
/// grouped surfaces.
#[derive(IntoElement)]
pub struct SectionHeader {
    title: SharedString,
    palette: Palette,
    debug_selector: Option<&'static str>,
}

impl SectionHeader {
    /// Build a section header from display-ready title text.
    pub fn new(title: impl Into<SharedString>, palette: Palette) -> Self {
        Self {
            title: title.into(),
            palette,
            debug_selector: None,
        }
    }

    /// Preserve a caller-owned selector for behavior/geometry probes.
    #[must_use]
    pub fn debug_selector(mut self, selector: &'static str) -> Self {
        self.debug_selector = Some(selector);
        self
    }

    /// Render the heading and divider.
    #[must_use]
    pub fn render(self) -> gpui::Div {
        let mut header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(tokens::SPACE_8)
            .child(
                div()
                    .text_size(tokens::FONT_14)
                    .text_color(self.palette.fg)
                    .font_weight(tokens::FONT_WEIGHT_SEMIBOLD.into())
                    .child(self.title),
            )
            .child(div().flex_grow().h(px(1.0)).bg(self.palette.border));
        if let Some(selector) = self.debug_selector {
            header = header.debug_selector(move || selector.to_string());
        }
        header
    }
}

impl RenderOnce for SectionHeader {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.render()
    }
}
