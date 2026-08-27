//! Shared card/panel surface primitive.

use gpui::{AnyElement, App, IntoElement, ParentElement, RenderOnce, Styled, Window, div};
use taskmanager_theme::{Color, Length, Palette, tokens};

/// A palette-owned card surface with configurable padding and fill.
#[derive(IntoElement)]
pub struct CardSurface {
    children: Vec<AnyElement>,
    palette: Palette,
    background: Color,
    padding: Length,
    radius: Length,
    bordered: bool,
}

impl CardSurface {
    /// Build an elevated surface using the palette's canonical card tokens.
    pub fn new(palette: Palette) -> Self {
        Self {
            children: Vec::new(),
            background: palette.surface,
            padding: tokens::SPACE_12,
            radius: palette.panel_radius,
            bordered: true,
            palette,
        }
    }

    /// Add a surface child.
    #[must_use]
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    /// Add several surface children.
    #[must_use]
    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.children
            .extend(children.into_iter().map(IntoElement::into_any_element));
        self
    }

    /// Set the inner padding.
    #[must_use]
    pub fn padding(mut self, padding: Length) -> Self {
        self.padding = padding;
        self
    }

    /// Set the surface fill when a lower/elevated semantic surface is needed.
    #[must_use]
    pub fn background(mut self, background: Color) -> Self {
        self.background = background;
        self
    }

    /// Set the skin-provided radius tier.
    #[must_use]
    pub fn radius(mut self, radius: Length) -> Self {
        self.radius = radius;
        self
    }

    /// Omit the hairline border while retaining the card geometry.
    #[must_use]
    pub fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }

    /// Render the surface as a concrete `Div`.
    #[must_use]
    pub fn render(self) -> gpui::Div {
        let mut surface = div()
            .p(self.padding)
            .rounded(self.radius)
            .bg(self.background)
            .children(self.children);
        if self.bordered {
            surface = surface.border_1().border_color(self.palette.border);
        }
        surface
    }
}

impl RenderOnce for CardSurface {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.render()
    }
}
