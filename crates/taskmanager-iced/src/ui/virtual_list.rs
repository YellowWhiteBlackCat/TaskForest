//! Shared viewport-aware window math for the Iced table surfaces.
//!
//! The process/application projections remain complete because keyboard
//! navigation and selection need the canonical row order. The expensive part
//! is the widget tree, though, so the large Iced tables only materialize
//! this bounded window plus a small overscan band. Fixed row extents make the
//! unmaterialized rows representable by spacers without measuring them.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use iced::widget::{Space, column, container, scrollable};
use iced::{Element, Length};

use crate::app::Message;

mod columns;
pub(crate) use columns::{ColumnWidth, TableColumn};

/// Extra rows kept above and below the viewport so a small wheel movement does
/// not rebuild the widget tree on every pixel of scrolling.
pub(crate) const OVERSCAN_ROWS: usize = 4;

/// Shared fixed header contract for the inventory tables. Applications and
/// App-history keep named aliases where their focus/visual code needs them.
pub(crate) const VIRTUAL_TABLE_HEADER_HEIGHT: f32 = 32.0;

/// A bounded row window and the spacer heights that preserve the full content
/// height inside an Iced `scrollable`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VirtualWindow {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) top: f32,
    pub(crate) bottom: f32,
}

impl VirtualWindow {
    /// Calculate the rows that need widgets for a fixed-height list.
    ///
    /// `prefix_height` is content that appears before the rows INSIDE the same
    /// scrollable (a header that scrolls in flow, as on the App-history page).
    /// Sticky-header tables have no prefix — see [`Self::for_sticky_rows`].
    /// The calculation clamps a hostile/stale scroll offset to the current
    /// content extent, so a filter that shrinks the list cannot produce an
    /// out-of-bounds slice.
    #[must_use]
    pub(crate) fn for_rows(
        row_count: usize,
        scroll_y: f32,
        viewport_height: f32,
        row_height: f32,
        prefix_height: f32,
    ) -> Self {
        if row_count == 0 {
            return Self {
                start: 0,
                end: 0,
                top: 0.0,
                bottom: 0.0,
            };
        }

        let row_height = finite_positive(row_height, 1.0);
        let prefix_height = finite_non_negative(prefix_height);
        let viewport_height = finite_positive(viewport_height, row_height);
        let content_height = prefix_height + row_count as f32 * row_height;
        let max_scroll = (content_height - viewport_height).max(0.0);
        let scroll_y = if scroll_y.is_finite() {
            scroll_y.max(0.0).min(max_scroll)
        } else {
            0.0
        };

        // The header/prefix consumes the first part of the viewport. Once it
        // has scrolled away, the row offset advances one fixed extent at a
        // time. Overscan is applied symmetrically around that row index.
        let row_scroll = (scroll_y - prefix_height).max(0.0);
        let first_visible = (row_scroll / row_height).floor() as usize;
        let start = first_visible.saturating_sub(OVERSCAN_ROWS);

        // Include enough rows for the whole viewport plus the prefix. The
        // extra overscan is intentional: it also covers the transition where
        // the header is partly visible without special-case row construction.
        let visible_rows = ((viewport_height + prefix_height) / row_height).ceil() as usize;
        let materialized = visible_rows.saturating_add(OVERSCAN_ROWS * 2 + 1);
        let end = start.saturating_add(materialized).min(row_count);

        Self {
            start,
            end,
            top: start as f32 * row_height,
            bottom: row_count.saturating_sub(end) as f32 * row_height,
        }
    }

    /// Row window for a sticky-header table body. The header row lives OUTSIDE
    /// the body scrollable, so the body's scroll content starts at the first
    /// row spacer with no prefix: offsets, clamping, and the keyboard reveal
    /// all operate on the pure row extent. Hostile offsets stay clamped by
    /// [`Self::for_rows`].
    #[must_use]
    pub(crate) fn for_sticky_rows(
        row_count: usize,
        scroll_y: f32,
        viewport_height: f32,
        row_height: f32,
    ) -> Self {
        Self::for_rows(row_count, scroll_y, viewport_height, row_height, 0.0)
    }

    /// Calculate a bounded horizontal item window using the same fixed-extent
    /// math as [`Self::for_rows`]. `top`/`bottom` represent leading/trailing
    /// horizontal spacer widths for this axis.
    #[must_use]
    pub(crate) fn for_columns(
        item_count: usize,
        scroll_x: f32,
        viewport_width: f32,
        item_width: f32,
        prefix_width: f32,
    ) -> Self {
        Self::for_rows(
            item_count,
            scroll_x,
            viewport_width,
            item_width,
            prefix_width,
        )
    }

    /// The integer portion used in lazy-widget invalidation keys. Spacer
    /// heights are derived from this pair and the row-height contract.
    #[must_use]
    pub(crate) const fn key(self) -> (usize, usize) {
        (self.start, self.end)
    }
}

/// Add the materialized row range to a renderer-specific lazy-body key.
/// Keeping this in one place prevents a new virtual table from accidentally
/// caching the first visible range forever.
#[must_use]
pub(crate) fn virtual_table_key(base: u64, window: VirtualWindow) -> u64 {
    let mut hasher = DefaultHasher::new();
    base.hash(&mut hasher);
    window.key().hash(&mut hasher);
    hasher.finish()
}

/// Build a virtual table body from only the requested row range. The caller's
/// closure receives `(start, end)` and must build exactly that bounded slice;
/// the helper owns the spacer geometry and the full content height contract.
#[must_use]
pub(crate) fn virtual_table_body<F>(
    window: VirtualWindow,
    width: Length,
    build_rows: F,
) -> Element<'static, Message, iced::Theme, iced::Renderer>
where
    F: FnOnce(usize, usize) -> Vec<Element<'static, Message, iced::Theme, iced::Renderer>>,
{
    let rows = build_rows(window.start, window.end);
    let mut children: Vec<Element<'static, Message, iced::Theme, iced::Renderer>> =
        Vec::with_capacity(rows.len() + 2);
    children.push(Space::new().height(Length::Fixed(window.top)).into());
    children.extend(rows);
    children.push(Space::new().height(Length::Fixed(window.bottom)).into());
    column(children).spacing(0).width(width).into()
}

/// Build a horizontal virtual body. The same `VirtualWindow` is used for
/// cards and compact device pills; only the spacer axis changes.
#[must_use]
pub(crate) fn virtual_horizontal_body<'a, F>(
    window: VirtualWindow,
    height: Length,
    build_items: F,
) -> Element<'a, Message, iced::Theme, iced::Renderer>
where
    F: FnOnce(usize, usize) -> Vec<Element<'a, Message, iced::Theme, iced::Renderer>>,
{
    let items = build_items(window.start, window.end);
    let mut children: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> =
        Vec::with_capacity(items.len() + 2);
    children.push(Space::new().width(Length::Fixed(window.top)).into());
    children.extend(items);
    children.push(Space::new().width(Length::Fixed(window.bottom)).into());
    iced::widget::row(children).spacing(0).height(height).into()
}

/// Apply the fixed row-height contract used by [`virtual_table_body`]. Keeping
/// this wrapper in the primitive makes newly migrated inventory tables share
/// the same spacer math and prevents a natural-height row from drifting the
/// viewport window.
#[must_use]
pub(crate) fn virtual_table_row(
    row: Element<'static, Message, iced::Theme, iced::Renderer>,
    row_height: f32,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    container(row)
        .height(Length::Fixed(row_height.max(1.0)))
        .width(Length::Fill)
        .into()
}

/// Compose the sticky header + virtual body + scrollable shell used by large
/// Iced tables. Iced's `scrollable` has no native sticky band, so the header
/// row is stacked OUTSIDE the body's scrollable: it never scrolls away
/// vertically, while column alignment is preserved because header and body
/// cells derive their widths from the same page column specs.
///
/// - `Vertical`: `column[header, scrollable(body)]` — the header stays fixed
///   above the body's vertical scrollable.
/// - `Both`: the body scrolls vertically under the fixed header while both
///   scroll horizontally in lockstep — an outer horizontal `scrollable` wraps
///   `column[header, inner vertical scrollable(body)]`, so the header rides
///   the same horizontal offset as the rows. The tracked scroll id, offset
///   reporting, and the vertical scrollbar therefore belong to the inner
///   body scrollable (the outer carries the horizontal axis only).
///
/// Row projection remains page-owned; viewport state, scroll identity,
/// direction and callback wiring are standardized here.
#[must_use]
pub(crate) fn virtual_table<'a>(
    id: iced::widget::Id,
    header: Element<'a, Message, iced::Theme, iced::Renderer>,
    body: Element<'a, Message, iced::Theme, iced::Renderer>,
    content_width: Length,
    direction: scrollable::Direction,
    on_scroll: impl Fn(scrollable::Viewport) -> Message + 'a,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    match direction {
        scrollable::Direction::Vertical(scrollbar) => {
            let body = scrollable(body)
                .id(id)
                .direction(scrollable::Direction::Vertical(scrollbar))
                .width(Length::Fill)
                .height(Length::Fill)
                .on_scroll(on_scroll);
            column![header, body]
                .spacing(0)
                .width(content_width)
                .height(Length::Fill)
                .into()
        }
        scrollable::Direction::Both {
            vertical,
            horizontal,
        } => {
            // The inner vertical scrollable must span the full fixed content
            // width: a Fill width would clip the trailing columns at the
            // viewport even after the outer scrolls to them.
            let body_width = match content_width {
                Length::Fixed(width) => Length::Fixed(width),
                Length::Fill | Length::Shrink | Length::FillPortion(_) => Length::Fill,
            };
            let body = scrollable(body)
                .id(id)
                .direction(scrollable::Direction::Vertical(vertical))
                .width(body_width)
                .height(Length::Fill)
                .on_scroll(on_scroll);
            scrollable(
                column![header, body]
                    .spacing(0)
                    .width(content_width)
                    .height(Length::Fill),
            )
            .direction(scrollable::Direction::Horizontal(horizontal))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        }
        scrollable::Direction::Horizontal(scrollbar) => {
            // No vertical axis: nothing for the header to stick against; the
            // shell degrades to a plain horizontal scroll of the stacked
            // header + body (no page currently uses this shape).
            scrollable(
                column![header, body]
                    .spacing(0)
                    .width(content_width)
                    .height(Length::Fill),
            )
            .direction(scrollable::Direction::Horizontal(scrollbar))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        }
    }
}

fn finite_positive(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/virtual_list_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/gui/ui/table_columns_tests.rs"]
mod table_columns_tests;
