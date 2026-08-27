//! Along-axis layout math for the variable-size virtual list (absorption
//! §5.4): the `sizes`/`origins` prefix-sum cache, the visible-range scan,
//! deferred scroll resolution and the `ItemSizeLayout` element-state cache.

use std::ops::Range;
use std::rc::Rc;

use gpui::{Axis, Bounds, Half, Pixels, Point, ScrollStrategy, Size, point, px, size};

use super::{DeferredScrollRequest, axis_is_horizontal, axis_is_vertical};

/// The along-axis prefix-sum cache (absorption §5.4 step 2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SizeLayout {
    /// Per-item along-axis size including the trailing gap (the last item
    /// has no gap).
    pub sizes: Vec<Pixels>,
    /// Cumulative origins: `origins[i]` is the start of item `i`.
    pub origins: Vec<Pixels>,
    /// Total along-axis extent of the content.
    pub content_size: Pixels,
}

/// Build the `sizes`/`origins` prefix-sum cache for the given item sizes.
#[must_use]
pub fn build_size_layout(item_sizes: &[Pixels], gap: Pixels) -> SizeLayout {
    let mut sizes = Vec::with_capacity(item_sizes.len());
    for (i, &size) in item_sizes.iter().enumerate() {
        sizes.push(if i + 1 == item_sizes.len() {
            size
        } else {
            size + gap
        });
    }
    let mut origins = Vec::with_capacity(sizes.len());
    let mut cumulative = px(0.0);
    for &size in &sizes {
        origins.push(cumulative);
        cumulative += size;
    }
    SizeLayout {
        sizes,
        origins,
        content_size: cumulative,
    }
}

/// Visible range scan (absorption §5.4 step 3, 附录 A-11 fix).
///
/// - `first` is the first item whose end crosses the viewport start edge;
/// - `last` (exclusive) is the first item whose end crosses the viewport far
///   edge — if no item crosses it, **all** remaining items are visible
///   (this replaces gc's `last == 0 → items_count` special case with one
///   uniform rule);
/// - when the whole content lies before the viewport start, an empty range
///   is returned (the caller's offset clamp fixes it on the next frame).
///
/// `offset` is negative while scrolled (gpui convention).
#[must_use]
pub fn visible_range_for(
    item_sizes: &[Pixels],
    offset: f32,
    viewport: f32,
    leading_padding: f32,
) -> Range<usize> {
    let count = item_sizes.len();
    if count == 0 || viewport <= 0.0 {
        return 0..0;
    }

    let start_threshold = -offset - leading_padding;
    let mut first = count;
    let mut cumulative = 0.0f32;
    for (i, size) in item_sizes.iter().enumerate() {
        cumulative += f32::from(size);
        if cumulative > start_threshold {
            first = i;
            break;
        }
    }
    if first == count {
        // Everything is above the viewport start.
        return count..count;
    }

    let end_threshold = -offset + viewport;
    let mut last = count;
    cumulative = 0.0;
    for (i, size) in item_sizes.iter().enumerate() {
        cumulative += f32::from(size);
        if cumulative > end_threshold {
            last = i + 1;
            break;
        }
    }
    first..last.max(first)
}

/// Resolve a deferred scroll request against measured item/content bounds
/// (absorption §5.4 step 2). `Top`/`Bottom` align the item's edge only when
/// it is out of view (non-strict); `Center` centers it when possible.
#[must_use]
pub fn resolve_deferred_scroll(
    axis: Axis,
    offset: Point<Pixels>,
    item_bounds: Bounds<Pixels>,
    content_bounds: &Bounds<Pixels>,
    request: DeferredScrollRequest,
) -> Point<Pixels> {
    match request.strategy {
        ScrollStrategy::Center => {
            if axis_is_vertical(axis) {
                point(
                    offset.x,
                    content_bounds.top() + content_bounds.size.height.half()
                        - item_bounds.top()
                        - item_bounds.size.height.half(),
                )
            } else {
                point(
                    content_bounds.left() + content_bounds.size.width.half()
                        - item_bounds.left()
                        - item_bounds.size.width.half(),
                    offset.y,
                )
            }
        }
        ScrollStrategy::Top => {
            if axis_is_vertical(axis) {
                if item_bounds.top() + offset.y < content_bounds.top()
                    || item_bounds.bottom() + offset.y > content_bounds.bottom()
                {
                    point(offset.x, content_bounds.top() - item_bounds.top())
                } else {
                    offset
                }
            } else if item_bounds.left() + offset.x < content_bounds.left()
                || item_bounds.right() + offset.x > content_bounds.right()
            {
                point(content_bounds.left() - item_bounds.left(), offset.y)
            } else {
                offset
            }
        }
        ScrollStrategy::Bottom => {
            if axis_is_vertical(axis) {
                if item_bounds.top() + offset.y < content_bounds.top()
                    || item_bounds.bottom() + offset.y > content_bounds.bottom()
                {
                    point(offset.x, content_bounds.bottom() - item_bounds.bottom())
                } else {
                    offset
                }
            } else if item_bounds.left() + offset.x < content_bounds.left()
                || item_bounds.right() + offset.x > content_bounds.right()
            {
                point(content_bounds.right() - item_bounds.right(), offset.y)
            } else {
                offset
            }
        }
    }
}

/// Element-state layout cache (absorption §5.4): rebuilt only when the item
/// sizes change (Rc pointer + content compare), per element id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemSizeLayout {
    pub(super) item_sizes: Rc<Vec<Pixels>>,
    layout: SizeLayout,
    cross_extent: Pixels,
    pub(super) content_size: Size<Pixels>,
    pub(super) last_layout_bounds: Bounds<Pixels>,
}

impl ItemSizeLayout {
    pub(super) fn rebuild(
        &mut self,
        item_sizes: Rc<Vec<Pixels>>,
        gap: Pixels,
        cross_extent: Pixels,
        axis: Axis,
    ) {
        self.item_sizes = item_sizes.clone();
        self.layout = build_size_layout(&item_sizes, gap);
        self.cross_extent = cross_extent;
        self.content_size = if axis_is_horizontal(axis) {
            size(self.layout.content_size, cross_extent)
        } else {
            size(cross_extent, self.layout.content_size)
        };
    }

    /// Per-item along-axis sizes (gap included).
    pub fn sizes(&self) -> &[Pixels] {
        &self.layout.sizes
    }

    /// Cumulative along-axis origins.
    pub fn origins(&self) -> &[Pixels] {
        &self.layout.origins
    }

    /// The scrollable content size (along axis = sum, cross axis = the
    /// measured uniform cross extent).
    pub fn content_size(&self) -> Size<Pixels> {
        self.content_size
    }
}
