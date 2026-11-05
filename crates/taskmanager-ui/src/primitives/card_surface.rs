//! Shared card/panel surface primitive (GLASS-01 frosted glass).

use gpui::{
    AbsoluteLength, AnyElement, App, DefiniteLength, IntoElement, ParentElement, RenderOnce,
    Styled, Window, div, rems,
};
use taskmanager_theme::{Color, GlassTier, Length, Palette, tokens};

/// A palette-owned card surface with configurable padding and fill.
#[derive(IntoElement)]
pub struct CardSurface {
    children: Vec<AnyElement>,
    palette: Palette,
    background: Color,
    padding: Length,
    radius: Length,
    bordered: bool,
    /// When set, the surface paints the frosted-glass contract for this tier:
    /// translucent theme gradient, a hairline edge, and the two-layer soft
    /// shadow. The palette's tokens remain the fallback for non-glass surfaces.
    glass: Option<GlassTier>,
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
            glass: Some(GlassTier::Card),
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

    /// Set an explicit surface fill and opt out of the glass material.
    ///
    /// Custom-fill call sites are opaque content tracks (dense rows, list
    /// cards) whose readability must not depend on the backdrop. Call
    /// [`Self::glass`] afterwards to make the custom fill a glass tier.
    #[must_use]
    pub fn background(mut self, background: Color) -> Self {
        self.background = background;
        self.glass = None;
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

    /// Render this surface as a frosted-glass panel of `tier`.
    #[must_use]
    pub fn glass(mut self, tier: GlassTier) -> Self {
        self.glass = Some(tier);
        self
    }

    /// The effective fill after applying the explicit-fill/glass precedence.
    #[must_use]
    pub(crate) fn fill(&self, palette: &Palette) -> Color {
        self.glass
            .map_or(self.background, |tier| tier.layer(&palette.theme).fill)
    }

    /// Render the surface as a concrete `Div`.
    #[must_use]
    pub fn render(self) -> gpui::Div {
        // GLASS-01: one glass authority. The theme's GlassLayer owns the
        // fill/edge/shadow contract; this primitive only maps it onto GPUI's
        // shadow vocabulary. `card_surface()` is the expected hue, so an HC or
        // opaque fallback resolves to the same paint the caller used to see.
        let glass = self.glass.map(|tier| tier.layer(&self.palette.theme));
        let radius: AbsoluteLength = if self.glass.is_some() {
            crate::theme_binding::rem_length(self.radius).into()
        } else {
            crate::theme_binding::absolute(self.radius)
        };
        let padding: DefiniteLength = if self.glass.is_some() {
            crate::theme_binding::rem_length(self.padding).into()
        } else {
            crate::theme_binding::definite_length(self.padding)
        };
        let background = self.glass.map_or_else(
            || crate::theme_binding::fill(self.fill(&self.palette)),
            |tier| crate::theme_binding::glass_fill(&self.palette.theme, tier),
        );
        let mut surface = div()
            .p(padding)
            .rounded(radius)
            .bg(background)
            .children(self.children);
        if self.bordered {
            surface = if let Some(tier) = self.glass {
                surface.border(rems(tier.edge_width_rem()))
            } else {
                surface.border_1()
            };
            surface = surface.border_color(crate::theme_binding::hsla(
                glass.map_or(self.palette.border, |layer| layer.edge),
            ));
        }
        if let Some(tier) = self.glass {
            surface = surface.shadow(crate::theme_binding::glass_shadow(
                &self.palette.theme,
                tier,
            ));
        }
        surface
    }
}

impl RenderOnce for CardSurface {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.render()
    }
}
