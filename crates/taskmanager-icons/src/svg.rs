//! [`IconId`] → GPUI SVG element rendering.

use gpui::{App, IntoElement, Refineable, RenderOnce, StyleRefinement, Styled, Window, svg};
use taskmanager_ui_contract::IconId;

use crate::path;

/// A GPUI icon element for a semantic [`IconId`].
///
/// Renders the embedded SVG asset through gpui's native `svg` element (no
/// `gpui_component::Icon`). The glyph color is resolved at layout time from the
/// composed ancestor text style, so an icon placed inside a `text_color(..)`
/// container inherits that color; callers may also chain `.text_color(..)`
/// directly. All GPUI style methods (`.size(..)`, `.w(..)`, …) are available on
/// this builder.
#[derive(IntoElement)]
pub struct Icon {
    id: IconId,
    style: StyleRefinement,
}

impl Icon {
    fn new(id: IconId) -> Self {
        Self {
            id,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for Icon {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let mut element = svg().path(path(self.id));
        element.style().refine(&self.style);
        // gpui's Svg element paints only when a text color is resolved (it
        // reads its own style.text.color, never a parent's). Inherit the
        // composed ancestor text style — the same fallback the gc Icon
        // rendering used (window.text_style().color at layout time).
        let has_own_color = element
            .style()
            .text
            .as_ref()
            .is_some_and(|text| text.color.is_some());
        if !has_own_color {
            element = element.text_color(window.text_style().color);
        }
        element
    }
}

/// Build a tintable GPUI icon. Color inherits from the surrounding text style.
#[must_use]
pub fn icon(icon: IconId) -> Icon {
    Icon::new(icon)
}
