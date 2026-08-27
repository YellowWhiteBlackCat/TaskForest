//! Variable-size virtual list (absorption §5.4-5.6).
//!
//! The algorithm assets: the along-axis `sizes`/`origins` prefix-sum cache,
//! the visible-range scan, deferred scroll resolution and the
//! `ItemSizeLayout` element-state cache. Unlike `gpui::uniform_list` the
//! items may have different along-axis sizes. Row virtualization of
//! [`super::table::Table`] uses `uniform_list` (uniform rows, gpui
//! primitive); other variable-size lists can use this primitive for
//! per-item widths. The shared Table computes one column range per frame
//! instead of nesting this list in every visible row.
//!
//! Defect fixes over gc (附录 A):
//! - A-11: the `last == 0` scan special-case is gone; [`visible_range_for`]
//!   uses one uniform rule (a scan that never crosses the far edge means all
//!   remaining items are visible). Boundary tests lock 1 item / exactly
//!   filled / oversized item.
//! - A-12: there is no `scroll_to_item(0, Top)` shortcut; every request goes
//!   through the deferred protocol.
//! - A-13: item measurement falls back to `Size::default()` when the render
//!   closure produces no item (gc could measure a default-size element and
//!   miscompute the whole list height).

use std::cell::RefCell;
use std::ops::{Deref, Range};
use std::rc::Rc;

use gpui::{
    Along, AnyElement, App, AvailableSpace, Axis, Bounds, ContentMask, Context, Div, Element,
    ElementId, Entity, GlobalElementId, Hitbox, InteractiveElement, IntoElement, IsZero,
    ListSizingBehavior, Pixels, Point, Render, ScrollHandle, ScrollStrategy, Size, Stateful,
    StatefulInteractiveElement, Styled, Window, div, point, px, size,
};

use crate::primitives::scrollbar::ScrollbarHandle;

mod layout;

pub use layout::{
    ItemSizeLayout, SizeLayout, build_size_layout, resolve_deferred_scroll, visible_range_for,
};

/// gpui 0.2.2's `Axis` has no direction predicates; keep the checks in one
/// place (gc's `AxisExt` equivalent).
#[inline]
pub(crate) fn axis_is_vertical(axis: Axis) -> bool {
    matches!(axis, Axis::Vertical)
}

#[inline]
pub(crate) fn axis_is_horizontal(axis: Axis) -> bool {
    !axis_is_vertical(axis)
}

/// A typed deferred scroll request (the handle's payload, fulfilled during
/// the next prepaint; absorption §5.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeferredScrollRequest {
    /// Flat item index to scroll to.
    pub item_index: usize,
    /// Where to place the item.
    pub strategy: ScrollStrategy,
    /// Extra items of offset from the target.
    pub offset_items: usize,
}

struct VirtualListScrollHandleState {
    axis: Axis,
    items_count: usize,
    deferred_scroll_to_item: Option<DeferredScrollRequest>,
}

/// A scroll handle for [`VirtualList`]: `scroll_to_item` writes a deferred
/// request that the list resolves against measured item bounds at prepaint
/// (never immediately), so callers can issue requests before any layout.
#[derive(Clone)]
pub struct VirtualListScrollHandle {
    state: Rc<RefCell<VirtualListScrollHandleState>>,
    base_handle: ScrollHandle,
}

impl Default for VirtualListScrollHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualListScrollHandle {
    /// Create an unbounded scroll handle.
    pub fn new() -> Self {
        VirtualListScrollHandle {
            state: Rc::new(RefCell::new(VirtualListScrollHandleState {
                axis: Axis::Vertical,
                items_count: 0,
                deferred_scroll_to_item: None,
            })),
            base_handle: ScrollHandle::default(),
        }
    }

    /// The underlying gpui scroll handle.
    pub fn base_handle(&self) -> &ScrollHandle {
        &self.base_handle
    }

    /// Request a deferred scroll to `ix` (fulfilled at prepaint).
    pub fn scroll_to_item(&self, ix: usize, strategy: ScrollStrategy) {
        self.scroll_to_item_with_offset(ix, strategy, 0);
    }

    /// Request a deferred scroll to `ix` with `offset_items` items of offset.
    pub fn scroll_to_item_with_offset(
        &self,
        ix: usize,
        strategy: ScrollStrategy,
        offset_items: usize,
    ) {
        self.state.borrow_mut().deferred_scroll_to_item = Some(DeferredScrollRequest {
            item_index: ix,
            strategy,
            offset_items,
        });
    }

    /// Scroll to the bottom of the list (last item, top-aligned).
    pub fn scroll_to_bottom(&self) {
        let items_count = self.state.borrow().items_count;
        self.scroll_to_item(items_count.saturating_sub(1), ScrollStrategy::Top);
    }

    /// The pending deferred scroll request, if any.
    pub fn deferred_scroll(&self) -> Option<DeferredScrollRequest> {
        self.state.borrow().deferred_scroll_to_item
    }
}

impl ScrollbarHandle for VirtualListScrollHandle {
    fn offset(&self) -> Point<Pixels> {
        self.base_handle.offset()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.base_handle.set_offset(offset);
    }

    fn max_offset(&self) -> Size<Pixels> {
        self.base_handle.max_offset()
    }

    fn viewport(&self) -> Bounds<Pixels> {
        self.base_handle.bounds()
    }
}

impl Deref for VirtualListScrollHandle {
    type Target = ScrollHandle;

    fn deref(&self) -> &Self::Target {
        &self.base_handle
    }
}

/// Create a vertical [`VirtualList`].
///
/// `item_sizes` are the along-axis sizes of every item (heights here);
/// `f` renders the visible `Range<usize>` of flat indices.
pub fn v_virtual_list<R, V>(
    view: Entity<V>,
    id: impl Into<ElementId>,
    item_sizes: Rc<Vec<Pixels>>,
    f: impl 'static + Fn(&mut V, Range<usize>, &mut Window, &mut Context<V>) -> Vec<R>,
) -> VirtualList
where
    R: IntoElement,
    V: Render,
{
    virtual_list(view, id, Axis::Vertical, item_sizes, f)
}

/// Create a horizontal [`VirtualList`] (column virtualization).
///
/// `item_sizes` are the widths of every item; `f` renders the visible
/// `Range<usize>` of flat indices.
pub fn h_virtual_list<R, V>(
    view: Entity<V>,
    id: impl Into<ElementId>,
    item_sizes: Rc<Vec<Pixels>>,
    f: impl 'static + Fn(&mut V, Range<usize>, &mut Window, &mut Context<V>) -> Vec<R>,
) -> VirtualList
where
    R: IntoElement,
    V: Render,
{
    virtual_list(view, id, Axis::Horizontal, item_sizes, f)
}

pub(crate) fn virtual_list<R, V>(
    view: Entity<V>,
    id: impl Into<ElementId>,
    axis: Axis,
    item_sizes: Rc<Vec<Pixels>>,
    f: impl 'static + Fn(&mut V, Range<usize>, &mut Window, &mut Context<V>) -> Vec<R>,
) -> VirtualList
where
    R: IntoElement,
    V: Render,
{
    let id: ElementId = id.into();
    let scroll_handle = VirtualListScrollHandle::new();
    let render_range = move |visible_range, window: &mut Window, cx: &mut App| {
        view.update(cx, |this, cx| {
            f(this, visible_range, window, cx)
                .into_iter()
                .map(|component| component.into_any_element())
                .collect()
        })
    };

    VirtualList {
        id: id.clone(),
        axis,
        base: div()
            .id(id)
            .size_full()
            .overflow_scroll()
            .track_scroll(&scroll_handle),
        scroll_handle,
        items_count: item_sizes.len(),
        item_sizes,
        render_items: Box::new(render_range),
        sizing_behavior: ListSizingBehavior::default(),
    }
}

/// A virtual list rendering only the visible items along its axis.
///
/// The scrollable content size comes from the `sizes`/`origins` cache; the
/// cross-axis extent is measured from the first rendered item (absorption
/// Render the visible range of items into elements.
pub type RangeRenderer =
    Box<dyn for<'a> Fn(Range<usize>, &'a mut Window, &'a mut App) -> Vec<AnyElement>>;

/// §5.4 step 1) with a `Size::default()` fallback when nothing renders.
pub struct VirtualList {
    id: ElementId,
    axis: Axis,
    base: Stateful<Div>,
    scroll_handle: VirtualListScrollHandle,
    items_count: usize,
    item_sizes: Rc<Vec<Pixels>>,
    render_items: RangeRenderer,
    sizing_behavior: ListSizingBehavior,
}

impl VirtualList {
    /// Track an external scroll handle (both reads and writes go through it).
    #[must_use]
    pub fn track_scroll(mut self, scroll_handle: &VirtualListScrollHandle) -> Self {
        self.base = self.base.track_scroll(scroll_handle);
        self.scroll_handle = scroll_handle.clone();
        self
    }

    /// Set the sizing behavior (default `Infer`).
    #[must_use]
    pub fn with_sizing_behavior(mut self, behavior: ListSizingBehavior) -> Self {
        self.sizing_behavior = behavior;
        self
    }

    /// Measure the first rendered item to obtain the uniform cross extent
    /// (absorption §5.4 step 1; A-13: empty render → default size).
    fn measure_item(
        &self,
        list_width: Option<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Size<Pixels> {
        if self.items_count == 0 {
            return Size::default();
        }

        let item_ix = 0;
        let mut items = (self.render_items)(item_ix..item_ix + 1, window, cx);
        let Some(mut item_to_measure) = items.pop() else {
            return Size::default();
        };
        let available_space = size(
            list_width.map_or(AvailableSpace::MinContent, AvailableSpace::Definite),
            AvailableSpace::MinContent,
        );
        item_to_measure.layout_as_root(available_space, window, cx)
    }
}

/// Frame state kept between prepaint and paint.
pub struct VirtualListFrameState {
    /// Visible items to be painted.
    items: Vec<AnyElement>,
    size_layout: ItemSizeLayout,
}

impl IntoElement for VirtualList {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for VirtualList {
    type RequestLayoutState = VirtualListFrameState;
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let rem_size = window.rem_size();
        let font_size = window.text_style().font_size.to_pixels(rem_size);
        let longest_item_size = self.measure_item(None, window, cx);
        let cross_extent = if axis_is_horizontal(self.axis) {
            longest_item_size.height
        } else {
            longest_item_size.width
        };
        let mut size_layout = ItemSizeLayout::default();
        let axis = self.axis;

        let layout_id = self.base.interactivity().request_layout(
            global_id,
            inspector_id,
            window,
            cx,
            |style, window, cx| {
                size_layout = window.with_element_state(
                    global_id.unwrap(),
                    |state: Option<ItemSizeLayout>, _window| {
                        let mut state = state.unwrap_or_default();
                        let gap = style.gap.along(axis).to_pixels(font_size.into(), rem_size);
                        if state.item_sizes != self.item_sizes {
                            state.rebuild(self.item_sizes.clone(), gap, cross_extent, axis);
                        }
                        (state.clone(), state)
                    },
                );

                let content_size = size_layout.content_size;
                match self.sizing_behavior {
                    ListSizingBehavior::Infer => {
                        window.with_text_style(style.text_style().cloned(), |window| {
                            window.request_measured_layout(style, {
                                move |known_dimensions, available_space, _, _| {
                                    let extent = |known: Option<Pixels>, space: AvailableSpace| {
                                        known.unwrap_or(match space {
                                            AvailableSpace::Definite(x) => x,
                                            AvailableSpace::MinContent
                                            | AvailableSpace::MaxContent => {
                                                content_size.along(axis)
                                            }
                                        })
                                    };
                                    Size {
                                        width: extent(
                                            known_dimensions.width,
                                            available_space.width,
                                        ),
                                        height: extent(
                                            known_dimensions.height,
                                            available_space.height,
                                        ),
                                    }
                                }
                            })
                        })
                    }
                    ListSizingBehavior::Auto => window
                        .with_text_style(style.text_style().cloned(), |window| {
                            window.request_layout(style, None, cx)
                        }),
                }
            },
        );

        (
            layout_id,
            VirtualListFrameState {
                items: Vec::new(),
                size_layout,
            },
        )
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        layout.size_layout.last_layout_bounds = bounds;

        let style = self
            .base
            .interactivity()
            .compute_style(global_id, None, window, cx);
        let border_widths = style.border_widths.to_pixels(window.rem_size());
        let paddings = style
            .padding
            .to_pixels(bounds.size.into(), window.rem_size());

        let item_sizes = layout.size_layout.sizes().to_vec();
        let item_origins = layout.size_layout.origins().to_vec();

        let content_bounds = Bounds::from_corners(
            bounds.origin
                + point(
                    border_widths.left + paddings.left,
                    border_widths.top + paddings.top,
                ),
            bounds.bottom_right()
                - point(
                    border_widths.right + paddings.right,
                    border_widths.bottom + paddings.bottom,
                ),
        );

        let items_bounds = item_origins
            .iter()
            .enumerate()
            .map(|(i, &origin)| {
                let item_size = item_sizes[i];
                Bounds {
                    origin: match self.axis {
                        Axis::Horizontal => point(content_bounds.left() + origin, px(0.0)),
                        Axis::Vertical => point(px(0.0), content_bounds.top() + origin),
                    },
                    size: match self.axis {
                        Axis::Horizontal => size(item_size, content_bounds.size.height),
                        Axis::Vertical => size(content_bounds.size.width, item_size),
                    },
                }
            })
            .collect::<Vec<_>>();

        let axis = self.axis;
        let content_size = layout.size_layout.content_size;

        {
            let mut scroll_state = self.scroll_handle.state.borrow_mut();
            scroll_state.axis = axis;
            scroll_state.items_count = self.items_count;
        }

        let mut scroll_offset = self.scroll_handle.offset();
        if let Some(request) = self.scroll_handle.deferred_scroll()
            && let Some(item_bounds) = items_bounds.get(request.item_index + request.offset_items)
        {
            scroll_offset = resolve_deferred_scroll(
                axis,
                scroll_offset,
                *item_bounds,
                &content_bounds,
                request,
            );
        }
        self.scroll_handle.set_offset(scroll_offset);

        // Clamp the offset to the scrollable range (content minus viewport).
        let clamped = scroll_offset
            .max(&point(
                content_bounds.size.width - content_size.width,
                content_bounds.size.height - content_size.height,
            ))
            .min(&point(px(0.0), px(0.0)));
        if clamped != scroll_offset {
            self.scroll_handle.set_offset(clamped);
            scroll_offset = clamped;
        }

        self.base.interactivity().prepaint(
            global_id,
            inspector_id,
            bounds,
            content_size,
            window,
            cx,
            |_style, _, hitbox, window, cx| {
                if self.items_count > 0 {
                    let min_scroll_offset =
                        content_bounds.size.along(axis) - content_size.along(axis);
                    if !scroll_offset.along(axis).is_zero()
                        && scroll_offset.along(axis) < min_scroll_offset
                    {
                        scroll_offset = if axis_is_horizontal(axis) {
                            point(min_scroll_offset, scroll_offset.y)
                        } else {
                            point(scroll_offset.x, min_scroll_offset)
                        };
                        self.scroll_handle.set_offset(scroll_offset);
                    }

                    let visible_range = visible_range_for(
                        &item_sizes,
                        f32::from(scroll_offset.along(axis)),
                        f32::from(content_bounds.size.along(axis)),
                        if axis_is_horizontal(axis) {
                            f32::from(paddings.left)
                        } else {
                            f32::from(paddings.top)
                        },
                    );

                    let items = (self.render_items)(visible_range.clone(), window, cx);
                    let content_mask = ContentMask { bounds };
                    window.with_content_mask(Some(content_mask), |window| {
                        for (mut item, ix) in items.into_iter().zip(visible_range.clone()) {
                            let item_origin = match axis {
                                Axis::Horizontal => {
                                    content_bounds.origin
                                        + point(item_origins[ix] + scroll_offset.x, scroll_offset.y)
                                }
                                Axis::Vertical => {
                                    content_bounds.origin
                                        + point(scroll_offset.x, item_origins[ix] + scroll_offset.y)
                                }
                            };

                            let available_space = match axis {
                                Axis::Horizontal => size(
                                    AvailableSpace::Definite(item_sizes[ix]),
                                    AvailableSpace::Definite(content_bounds.size.height),
                                ),
                                Axis::Vertical => size(
                                    AvailableSpace::Definite(content_bounds.size.width),
                                    AvailableSpace::Definite(item_sizes[ix]),
                                ),
                            };

                            item.layout_as_root(available_space, window, cx);
                            item.prepaint_at(item_origin, window, cx);
                            layout.items.push(item);
                        }
                    });
                }

                hitbox
            },
        )
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.base.interactivity().paint(
            global_id,
            inspector_id,
            bounds,
            hitbox.as_ref(),
            window,
            cx,
            |_, window, cx| {
                for item in &mut layout.items {
                    item.paint(window, cx);
                }
            },
        )
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_data_virtual_list_tests.rs"]
mod tests;
