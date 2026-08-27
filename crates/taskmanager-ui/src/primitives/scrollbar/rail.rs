//! Pinned vertical scrollbar rail: a 12px hit strip + thumb shared by
//! scrollable pages and dialogs. Page-level hosts opt into the overlay mode:
//! the rail paints over content without reserving layout width and the thumb
//! stays invisible until the user scrolls or hovers (Mission Center / GTK4
//! overlay-scrollbar behavior). Bounded dialogs keep the legacy always-visible
//! rail and reserve their width through the shared viewport padding.
//!
//! Hosts overlay this on a `relative` scroll viewport and pass the same
//! scroll handle the viewport tracks. Debug selectors stay caller-owned:
//! page and dialog tests address the wrapper (and optionally the legacy thin
//! track selector) by their own keys.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    Styled, Window, div, px,
};
use taskmanager_theme::{Palette, tokens};

use super::{SCROLLBAR_WIDTH, Scrollbar, ScrollbarHandle, ScrollbarShow};

/// A right-edge vertical scrollbar rail: a `SCROLLBAR_WIDTH` hit strip
/// pinned to the parent's top/right/bottom, a 1px visual track centered
/// inside it (inset `tokens::SPACE_4` top and bottom), and an
/// always-visible [`Scrollbar`] thumb driven by `handle`.
#[derive(IntoElement)]
pub struct ScrollbarRail {
    id: &'static str,
    debug_selector: &'static str,
    track_debug_selector: Option<&'static str>,
    handle: Rc<dyn ScrollbarHandle>,
    palette: Palette,
    show: ScrollbarShow,
}

impl ScrollbarRail {
    /// Build a rail for `handle`. `id` names the wrapper element and derives
    /// the thumb's element id (`"{id}-thumb"`); `debug_selector` keys the
    /// wrapper's `debug_bounds` lookup.
    pub fn vertical(
        id: &'static str,
        debug_selector: &'static str,
        handle: Rc<dyn ScrollbarHandle>,
        palette: Palette,
    ) -> Self {
        Self {
            id,
            debug_selector,
            track_debug_selector: None,
            handle,
            palette,
            // Keep the legacy always-visible rail for bounded dialogs and
            // pinned tables; page-level hosts opt into the overlay fade via
            // [`Self::show`].
            show: ScrollbarShow::Always,
        }
    }

    /// Override the show mode (e.g. `Scrolling` for an overlay rail that
    /// fades in on activity and fades out when idle).
    #[must_use]
    pub fn show(mut self, show: ScrollbarShow) -> Self {
        self.show = show;
        self
    }

    /// Key the 1px visual track's `debug_bounds` lookup. Hosts that assert
    /// track geometry in tests opt in; the default registers none.
    #[must_use]
    pub fn track_debug_selector(mut self, selector: &'static str) -> Self {
        self.track_debug_selector = Some(selector);
        self
    }
}

/// The thumb element id derived from the wrapper id: the scroll element's
/// drag/fade state keys off it, so the suffix is part of the rail contract.
fn thumb_id(id: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{id}-thumb")))
}

impl RenderOnce for ScrollbarRail {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let Self {
            id,
            debug_selector,
            track_debug_selector,
            handle,
            palette,
            show,
        } = self;

        // Always-mode rails keep the legacy hairline track. Overlay rails
        // paint only the thumb (GTK4/libadwaita style), so their track stays
        // addressable for geometry tests but is invisible.
        let track_alpha = if show == ScrollbarShow::Always {
            0.18
        } else {
            0.0
        };
        let mut track = div()
            .absolute()
            .top(tokens::SPACE_4)
            .bottom(tokens::SPACE_4)
            .right(px((SCROLLBAR_WIDTH - 1.0) / 2.0))
            .w(px(1.0))
            .rounded_full()
            .bg(palette.border.with_alpha(track_alpha))
            .when(show != ScrollbarShow::Always, |track| track.opacity(0.0));
        if let Some(selector) = track_debug_selector {
            track = track.debug_selector(move || selector.to_string());
        }

        div()
            .id(id)
            .debug_selector(move || debug_selector.to_string())
            // A rail must block clicks from reaching content underneath, but
            // it must remain transparent to wheel dispatch. `occlude()` uses
            // BlockMouse and therefore removes the underlying scroll hitbox
            // from GPUI's scroll hit-test entirely; the narrower
            // BlockMouseExceptScroll behavior is the intended overlay model.
            .block_mouse_except_scroll()
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .w(px(SCROLLBAR_WIDTH))
            .child(track)
            .child(
                // The thumb paint already applies THUMB_INSET at both ends.
                // Keep its geometry on the full hit axis so the painted cap
                // aligns with the hairline track inset above instead of being
                // inset twice and stopping short at the bottom.
                Scrollbar::vertical(thumb_id(id), handle, palette)
                    .track_inset(px(0.0))
                    .show(show),
            )
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/ui_primitives_scrollbar_rail_tests.rs"]
mod tests;
