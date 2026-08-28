//! Shared layout contracts for the owned GPUI component layer.
//!
//! These helpers deliberately contain no page state or platform knowledge. A
//! page decides what it renders; this module owns the flex boundaries that let
//! that content coexist with the shell at narrow and wide window sizes.

pub mod adaptive_grid;

pub use adaptive_grid::AdaptiveGrid;

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Div, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    ScrollHandle, Stateful, StatefulInteractiveElement, Styled, div, px,
};
use taskmanager_theme::{Length, Palette, tokens};

use crate::primitives::scrollbar::rail::ScrollbarRail;
use crate::primitives::scrollbar::{ScrollbarHandle, ScrollbarShow};

/// The bounded flex viewport shared by every top-level page.
///
/// The extra inner boundary is intentional: it gives the page content a
/// definite shrinkable child, so intrinsic-width descendants cannot resize the
/// root column or push a sibling region outside the window.
#[must_use]
pub fn page_viewport(body: Div) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        .child(body.flex_1().min_w(px(0.0)).min_h(px(0.0)).w_full())
}

/// A padded page content frame.
///
/// The frame owns the common available-space contract and page padding. Page
/// renderers should only add their semantic content inside it.
#[must_use]
pub fn page_frame(body: impl IntoElement, padding: Pixels) -> Div {
    div()
        .flex_1()
        .min_h(px(0.0))
        .w_full()
        .min_w(px(0.0))
        .p(padding)
        .flex()
        .flex_col()
        .child(page_content_slot(body))
}

/// The reusable padded content frame used inside a page scaffold.
pub struct PageFrame {
    body: AnyElement,
    padding: Pixels,
    right_padding: Option<Pixels>,
}

impl PageFrame {
    /// Build a frame around one page content slot.
    pub fn new(body: impl IntoElement, padding: Pixels) -> Self {
        Self {
            body: body.into_any_element(),
            padding,
            right_padding: None,
        }
    }

    /// Override the trailing inset for pages whose own pinned rail is the
    /// right-edge chrome. This keeps the rail flush with the page boundary
    /// while the tracked viewport still reserves its hit width internally.
    #[must_use]
    pub fn right_padding(mut self, padding: Pixels) -> Self {
        self.right_padding = Some(padding);
        self
    }

    /// Render the bounded padded content region.
    #[must_use]
    pub fn render(self) -> Div {
        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .min_w(px(0.0))
            .p(self.padding)
            .when_some(self.right_padding, |frame, padding| frame.pr(padding))
            .flex()
            .flex_col()
            .child(page_content_slot(self.body))
    }
}

/// A complete page column with one padded body and an optional unpadded footer.
///
/// Status bars and similar shell-owned footers now share one placement rule,
/// so individual pages do not grow slightly different outer flex wrappers.
pub struct PageScaffold {
    frame: PageFrame,
    footer: Option<AnyElement>,
}

impl PageScaffold {
    /// Build a page scaffold from its body and page padding.
    pub fn new(body: impl IntoElement, padding: Pixels) -> Self {
        Self {
            frame: PageFrame::new(body, padding),
            footer: None,
        }
    }

    /// Add an optional shell footer such as the page status bar.
    #[must_use]
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Render the full-size page column.
    #[must_use]
    pub fn render(self) -> Div {
        let mut scaffold = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .w_full()
            // The data-page family's ONE outer shell (ADR-041): the
            // render-path guard proves every non-chart page paints through
            // this selector, so a skeleton adjustment propagates to all of
            // them from this single place.
            .debug_selector(|| "tm-page-scaffold".to_string())
            .child(self.frame.render());
        if let Some(footer) = self.footer {
            scaffold = scaffold.child(
                div()
                    .flex_shrink_0()
                    .min_w(px(0.0))
                    .w_full()
                    .debug_selector(|| "tm-page-scaffold-footer".to_string())
                    .child(footer),
            );
        }
        scaffold
    }
}

/// A bounded vertical scroll viewport.
///
/// The caller owns the `ScrollHandle` so scroll position remains page state;
/// this helper owns the flex/min-size contract that makes scrolling work
/// inside a page or panel without letting intrinsic content escape sideways.
#[must_use]
pub fn scroll_region(
    id: &'static str,
    scroll: ScrollHandle,
    body: impl IntoElement,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        .overflow_y_scroll()
        .track_scroll(&scroll)
        .child(scroll_content(body))
}

/// A scroll viewport for short-lived or locally stateful surfaces that do not
/// expose a persistent `ScrollHandle` to the caller.
#[must_use]
pub fn auto_scroll_region(id: &'static str, body: impl IntoElement) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        .overflow_y_scroll()
        .child(scroll_content(body))
}

/// A vertical scroll viewport whose content fills the available height when
/// it is shorter than the viewport, while retaining its intrinsic height when
/// it becomes taller. This is the page-level contract for sparse detail views:
/// a chart can consume unused space without disabling scrolling for a disk
/// partition list, a GPU engine block, or another optional footer.
#[must_use]
pub fn auto_scroll_region_fill(id: &'static str, body: impl IntoElement) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        .overflow_y_scroll()
        .child(fill_scroll_content(body))
}

/// A bounded vertical scroll slot for dialogs, cards, and other nested
/// surfaces. Unlike [`scroll_region`], this variant does not claim the
/// parent's remaining height; its `max_height` is the explicit viewport
/// contract and the content retains intrinsic height for a real scroll range.
#[must_use]
pub fn bounded_scroll_region(
    id: &'static str,
    max_height: Pixels,
    body: impl IntoElement,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        .max_h(max_height)
        .flex_shrink()
        .overflow_y_scroll()
        .child(scroll_content(body))
}

/// A bounded vertical scroll slot backed by a caller-owned handle.
#[must_use]
pub fn bounded_scroll_region_with_handle(
    id: &'static str,
    max_height: Pixels,
    scroll: ScrollHandle,
    body: impl IntoElement,
) -> Stateful<Div> {
    bounded_scroll_region(id, max_height, body).track_scroll(&scroll)
}

/// A bounded vertical scroll slot with a pinned, always-visible scrollbar
/// rail. The separate selectors let dialogs expose their viewport, rail, and
/// thin visual track independently to geometry tests.
pub struct BoundedScrollRailSpec {
    pub id: &'static str,
    pub viewport_selector: &'static str,
    pub scrollbar_id: &'static str,
    pub scrollbar_selector: &'static str,
    pub track_selector: &'static str,
    /// Exact viewport width for dialog bodies whose content must not grow the
    /// panel through its intrinsic text width. `None` fills the parent slot.
    pub width: Option<Pixels>,
    pub max_height: Pixels,
    pub scroll: ScrollHandle,
    pub palette: Palette,
}

/// Build a bounded scroll slot from one explicit geometry/identity contract.
#[must_use]
pub fn bounded_scroll_region_with_rail(
    spec: BoundedScrollRailSpec,
    body: impl IntoElement,
) -> Stateful<Div> {
    let BoundedScrollRailSpec {
        id,
        viewport_selector,
        scrollbar_id,
        scrollbar_selector,
        track_selector,
        width,
        max_height,
        scroll,
        palette,
    } = spec;
    let viewport = tracked_vertical_viewport(
        id,
        viewport_selector,
        scroll.clone(),
        true,
        scroll_content(body),
    );
    div()
        .id((ElementId::from(id), "frame"))
        .relative()
        .flex()
        .flex_col()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        .when_some(width, |frame, width| frame.w(width).max_w(width))
        .max_h(max_height)
        .flex_shrink()
        .child(viewport)
        .child(
            ScrollbarRail::vertical(scrollbar_id, scrollbar_selector, Rc::new(scroll), palette)
                .track_debug_selector(track_selector),
        )
}

/// Compose fixed dialog chrome above a bounded body with a pinned rail.
///
/// The fixed slot is deliberately outside the tracked viewport, so actions,
/// filters, or summary text never consume the scroll offset. The exact width
/// belongs to the shared column as well as the body; this prevents intrinsic
/// header or body text from independently widening the dialog.
#[must_use]
pub fn bounded_scroll_column_with_fixed_header(
    spec: BoundedScrollRailSpec,
    gap: Length,
    header: impl IntoElement,
    body: impl IntoElement,
) -> Div {
    let width = spec.width;
    div()
        .flex()
        .flex_col()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        .when_some(width, |column, width| {
            column.w(width).min_w(width).max_w(width)
        })
        .gap(gap)
        .child(div().flex_none().min_w(px(0.0)).w_full().child(header))
        .child(bounded_scroll_region_with_rail(spec, body))
}

/// A scroll viewport with the owned pinned scrollbar rail.
///
/// This is the page-level variant for surfaces where a visible, stable rail
/// is part of the layout contract. The body remains a slot, so the component
/// does not know whether it contains a list, cards, or a detail projection.
#[must_use]
pub fn scroll_region_with_rail(
    id: &'static str,
    viewport_selector: &'static str,
    scrollbar_id: &'static str,
    scrollbar_selector: &'static str,
    scroll: ScrollHandle,
    palette: Palette,
    body: impl IntoElement,
) -> Stateful<Div> {
    let viewport = tracked_vertical_viewport(
        id,
        viewport_selector,
        scroll.clone(),
        true,
        // Page-level rails must keep sparse content elastic: a graph or
        // dashboard card should consume the available height before the
        // viewport becomes scrollable. The non-shrinking basis still preserves
        // overflow when optional panels make the page taller than its slot.
        fill_scroll_content(body),
    );
    div()
        .id((ElementId::from(id), "frame"))
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        .child(viewport)
        .child(ScrollbarRail::vertical(
            scrollbar_id,
            scrollbar_selector,
            Rc::new(scroll),
            palette,
        ))
}

/// A page-level scroll region with a GTK4-style overlay rail.
///
/// Unlike [`scroll_region_with_rail`], the viewport does not reserve a right
/// inset for the rail: the 12px hit strip floats over the content and the
/// thumb is invisible until the user scrolls or hovers. This is the narrow
/// sidebar variant requested by the product (Mission Center sidebar), and it
/// must not be applied to wide page bodies — those keep their reserved rail
/// so text and stat columns cannot run underneath the scrollbar.
#[must_use]
pub fn scroll_region_with_overlay_rail(
    id: &'static str,
    viewport_selector: &'static str,
    scrollbar_id: &'static str,
    scrollbar_selector: &'static str,
    scroll: ScrollHandle,
    palette: Palette,
    body: impl IntoElement,
) -> Stateful<Div> {
    let viewport = tracked_vertical_viewport(
        id,
        viewport_selector,
        scroll.clone(),
        false,
        fill_scroll_content(body),
    );
    div()
        .id((ElementId::from(id), "frame"))
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        .child(viewport)
        .child(
            ScrollbarRail::vertical(scrollbar_id, scrollbar_selector, Rc::new(scroll), palette)
                .show(ScrollbarShow::Scrolling),
        )
}

/// The only node that consumes a vertical `ScrollHandle` offset.
///
/// Pinned chrome must remain outside this node: GPUI applies a tracked div's
/// offset to every child during prepaint, including absolutely positioned
/// descendants. Keeping the rail as a sibling in the two public wrappers above
/// makes its window-space bounds independent from the scrolling content.
fn tracked_vertical_viewport(
    id: &'static str,
    viewport_selector: &'static str,
    scroll: ScrollHandle,
    reserve_rail: bool,
    body: impl IntoElement,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        .debug_selector(move || viewport_selector.to_string())
        .when(reserve_rail, |viewport| viewport.pr(tokens::SPACE_16))
        .overflow_y_scroll()
        .track_scroll(&scroll)
        .child(body)
}

/// Pin a scrollbar rail to a caller-owned scrolling surface whose concrete
/// handle is not GPUI's `ScrollHandle` (for example a uniform-list handle).
/// The body is responsible for tracking that handle; this wrapper owns the
/// shared relative/rail geometry only.
#[must_use]
pub fn pinned_scroll_region(
    id: &'static str,
    debug_selector: &'static str,
    scrollbar_id: &'static str,
    handle: Rc<dyn ScrollbarHandle>,
    palette: Palette,
    body: impl IntoElement,
) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        .debug_selector(move || debug_selector.to_string())
        .child(page_content_slot(body))
        .child(ScrollbarRail::vertical(
            scrollbar_id,
            debug_selector,
            handle,
            palette,
        ))
}

/// Keep scroll content at its intrinsic height. Without this non-shrinking
/// boundary, a flex-column content slot can collapse to the viewport and leave
/// the parent scroll handle with no positive scroll range.
fn scroll_content(body: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .min_w(px(0.0))
        .w_full()
        .child(body)
}

/// Let a page body grow into the scroll viewport when it is short, but never
/// shrink intrinsic content that must remain reachable by scrolling. The
/// `flex_auto + flex_shrink_0` pairing is intentional: grow fills spare room;
/// the non-shrinking intrinsic basis preserves a real overflow range.
fn fill_scroll_content(body: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_auto()
        .flex_shrink_0()
        .min_w(px(0.0))
        .w_full()
        .child(body)
}

/// Give a page-owned element a definite, shrinkable flex slot.
///
/// `AnyElement` cannot be styled after it has crossed a component boundary, so
/// every page frame adds this small owned wrapper. It is the boundary that
/// turns the automatic minimum size into zero without asking each page to
/// repeat the recipe.
fn page_content_slot(body: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        .child(body)
}

#[cfg(test)]
#[path = "../tests/gui/ui_layout_tests.rs"]
mod tests;
