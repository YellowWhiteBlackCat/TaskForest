//! Width-adaptive card layout built from GPUI's flex-wrap contract.

use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, RenderOnce, Styled, Window,
    div, px,
};
use taskmanager_theme::{Length, tokens};

/// A wrapping row of equal-priority content slots.
///
/// `min_item_width` is a soft minimum: it is the preferred basis used to decide
/// when another item belongs on the next row, while the item can still shrink
/// below that basis on a very narrow viewport. That combination keeps a single
/// card inside the viewport instead of creating horizontal overflow at the
/// exact width where a page needs to remain usable.
#[derive(IntoElement)]
pub struct AdaptiveGrid {
    children: Vec<AnyElement>,
    min_item_width: gpui::Pixels,
    gap: Length,
    debug_selector: Option<&'static str>,
}

impl AdaptiveGrid {
    /// Build a card grid using the standard card rhythm.
    pub fn new(min_item_width: gpui::Pixels) -> Self {
        Self {
            children: Vec::new(),
            min_item_width,
            gap: tokens::SPACE_12,
            debug_selector: None,
        }
    }

    /// Add one card or content slot.
    #[must_use]
    pub fn child(mut self, child: impl IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    /// Add several cards or content slots.
    #[must_use]
    pub fn children(mut self, children: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.children
            .extend(children.into_iter().map(IntoElement::into_any_element));
        self
    }

    /// Override the inter-card gap with an existing theme spacing token.
    #[must_use]
    pub fn gap(mut self, gap: Length) -> Self {
        self.gap = gap;
        self
    }

    /// Preserve a caller-owned selector for geometry probes.
    #[must_use]
    pub fn debug_selector(mut self, selector: &'static str) -> Self {
        self.debug_selector = Some(selector);
        self
    }

    /// Render the adaptive grid as a concrete `Div`.
    #[must_use]
    pub fn render(self) -> gpui::Div {
        let min_item_width = self.min_item_width.max(px(0.0));
        let gap = Length(self.gap.0.max(0.0));
        let mut grid = div()
            .flex()
            .flex_row()
            .flex_wrap()
            // The grid is content-sized in the cross axis. It is commonly a
            // child of a page's vertical flex scroller; allowing that parent
            // to shrink this slot to zero makes every content-only card
            // disappear before it reaches the paint pass.
            .flex_shrink_0()
            .w_full()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .gap(crate::theme_binding::definite_length(gap));

        if let Some(selector) = self.debug_selector {
            grid = grid.debug_selector(move || selector.to_string());
        }

        for child in self.children {
            grid = grid.child(
                div()
                    .flex_grow()
                    .flex_shrink()
                    .flex_basis(min_item_width)
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    // Keep the caller's surface as the direct flex item.
                    // An extra min-height-zero column here makes GPUI's
                    // intrinsic measurement pass treat content-only cards as
                    // zero-height when the grid itself is inside a scroll
                    // column. The item still owns the width constraints, and
                    // callers can opt into their own inner flex layout.
                    .child(child),
            );
        }

        grid
    }
}

impl Default for AdaptiveGrid {
    fn default() -> Self {
        Self::new(px(240.0))
    }
}

impl RenderOnce for AdaptiveGrid {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.render()
    }
}
