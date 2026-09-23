//! Pointer projection and crosshair interaction for performance graphs.
//!
//! Keeping this stateful layer separate from the canvas painter prevents the
//! graph's rendering geometry and its event plumbing from growing together.
//! Both tooltip and crosshair still consume the exact same sample-slot map.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use gpui::{
    AnyElement, App, Bounds, ElementId, Hsla, InteractiveElement, IntoElement, MouseMoveEvent,
    ParentElement, Pixels, Point, Rgba, StatefulInteractiveElement, Styled, Window, canvas, div,
    fill, point, px, size,
};

use super::scene_cache::{GraphPaintContext, paint_graph_dual_scene, paint_graph_scene};
use super::slide::slide_progress;
use super::{
    DualGraphSeries, GraphCacheHandle, GraphOpts, GraphSettings, graph_slide_spacing,
    graph_slide_supported, sample_x, sample_x_slide, stroke_path,
};

/// A live graph hover: the window-space cursor position plus the formatted
/// value at that cursor. It lives in the owning window's RootView slot.
#[derive(Clone, Debug)]
pub struct GraphHover {
    /// Window-space pointer position (the tooltip anchor).
    pub cursor: Point<Pixels>,
    /// Caller-formatted value text, e.g. `"73%"` or `"4.2 GiB"`.
    pub text: String,
}

/// The second series of a two-series graph, as the hover element owns it:
/// its window-limited samples, tint color, and the label the tooltip and
/// legend print for the direction (e.g. "Write", "Send").
#[derive(Clone)]
pub struct GraphSecondarySeries {
    pub samples: Rc<[f32]>,
    pub base: Rgba,
    pub label: String,
}

/// The tooltip text at one sample slot of a two-series graph: each series
/// that holds a finite value at the index contributes `"<label> <value>"`,
/// joined by `" · "` (iced `multi_readout_text` parity). `None` when neither
/// series has evidence there — the tooltip is suppressed, never a fabricated
/// zero. Pure so the composition is unit-tested headlessly.
#[must_use]
pub(crate) fn multi_series_hover_text(
    primary_label: &str,
    primary: &[f32],
    secondary: Option<(&str, &[f32])>,
    index: usize,
    format_value: &dyn Fn(f32) -> String,
) -> Option<String> {
    let mut parts = Vec::with_capacity(2);
    for (label, samples) in std::iter::once((primary_label, primary)).chain(secondary) {
        if let Some(&value) = samples.get(index).filter(|value| value.is_finite()) {
            parts.push(format!("{label} {}", format_value(value)));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" \u{b7} "))
    }
}

/// Read a hover slot without holding its `RefCell` borrow across element
/// construction. Pages use the owned pair to place one unclipped tooltip.
pub fn graph_hover(slot: &Rc<RefCell<Option<GraphHover>>>) -> Option<(Point<Pixels>, String)> {
    slot.borrow()
        .as_ref()
        .map(|hover| (hover.cursor, hover.text.clone()))
}

/// Resolve the nearest sample slot using the same right-anchored slot grid as
/// `sample_x`. This is shared by the tooltip and painted crosshair, so sparse
/// windows cannot report a value from the opposite side.
pub(super) fn sample_index_at_cursor_x(
    samples: &[f32],
    left: Pixels,
    width: Pixels,
    x: Pixels,
    capacity: usize,
) -> Option<usize> {
    sample_slot_at_cursor_x(samples, left, width, x, capacity)
        .filter(|&index| samples[index].is_finite())
}

/// Positional slot resolution without the finiteness filter: a two-series
/// graph keeps the crosshair alive when one direction holds a gap at the
/// slot and the other holds evidence.
fn sample_slot_at_cursor_x(
    samples: &[f32],
    left: Pixels,
    width: Pixels,
    x: Pixels,
    capacity: usize,
) -> Option<usize> {
    let n = samples.len();
    if n == 0 || width <= px(0.0) {
        return None;
    }
    let rel = ((f32::from(x) - f32::from(left)) / f32::from(width)).clamp(0.0, 1.0);
    let denom = n.saturating_sub(1).max(capacity.saturating_sub(1)).max(1) as f32;
    let idx = if n == 1 {
        0
    } else {
        (n as f32 - 1.0 - (1.0 - rel) * denom)
            .round()
            .clamp(0.0, (n - 1) as f32) as usize
    };
    Some(idx)
}

/// Slide-aware cursor resolution: the visible curve is the slide base
/// translated left by `progress` slot widths, so the cursor-to-sample map
/// must use the same slot grid instead of the settled `sample_x` map.
///
/// Positional slide-aware slot resolution (see [`sample_slot_at_cursor_x`]).
fn sample_slot_at_cursor_x_slide(
    samples: &[f32],
    left: Pixels,
    width: Pixels,
    x: Pixels,
    data_points: usize,
    progress: f32,
) -> Option<usize> {
    let n = samples.len();
    if n == 0 || width <= px(0.0) {
        return None;
    }
    let capacity = GraphSettings::clamp_data_points(data_points).max(1);
    let denom = capacity.saturating_sub(1).max(1) as f32;
    let slot = f32::from(width) / denom;
    let index = ((f32::from(x) - f32::from(left)) / slot + progress.clamp(0.0, 1.0))
        .round()
        .clamp(0.0, (n - 1) as f32) as usize;
    Some(index)
}

/// One frame's crosshair projection: the canvas and pointer geometry plus the
/// series ink the crosshair marks. The owning hover element already holds the
/// samples/color/opts, so this view borrows them for the paint pass.
struct GraphCrosshair<'a> {
    /// Canvas bounds in window space.
    bounds: Bounds<Pixels>,
    /// Primary series samples.
    samples: &'a [f32],
    /// Primary (family) color.
    base: Rgba,
    /// Geometry and scale tunables.
    opts: GraphOpts,
    /// Window-space pointer position, or `None` when no hover is active.
    cursor: Option<Point<Pixels>>,
    /// Slide progress while the refresh animation is in flight.
    slide: Option<f32>,
    /// Secondary series samples plus its tint color, sharing the slot grid.
    secondary: Option<(&'a [f32], Rgba)>,
}

/// Paint the active graph's focus crosshair after the area/stroke. The vertical
/// rule follows the pointer; each series that holds a finite sample at the
/// resolved slot gets its own horizontal rule and snap dot in its color, so a
/// gap in one direction never fabricates a mark for it.
fn draw_graph_crosshair(window: &mut Window, crosshair: GraphCrosshair<'_>) {
    let GraphCrosshair {
        bounds,
        samples,
        base,
        opts,
        cursor,
        slide,
        secondary,
    } = crosshair;
    let Some(cursor) = cursor else {
        return;
    };
    let (slide, slide_progress) = slide.map_or((false, 0.0), |progress| (true, progress));
    let left = bounds.origin.x;
    let top = bounds.origin.y;
    let width = bounds.size.width;
    let height = bounds.size.height;
    let right = left + width;
    let bottom = top + height;
    if cursor.x < left || cursor.x > right || cursor.y < top || cursor.y > bottom {
        return;
    }
    // Slot resolution: a single-series graph suppresses the crosshair on a
    // gap slot exactly as before; a two-series graph keeps it as long as
    // EITHER direction has evidence there.
    let slot = if slide {
        sample_slot_at_cursor_x_slide(
            samples,
            left,
            width,
            cursor.x,
            opts.data_points,
            slide_progress,
        )
    } else {
        sample_slot_at_cursor_x(samples, left, width, cursor.x, opts.data_points)
    };
    let index = slot.filter(|&index| match secondary {
        None => samples[index].is_finite(),
        Some((secondary_samples, _)) => {
            samples[index].is_finite()
                || secondary_samples
                    .get(index)
                    .is_some_and(|value| value.is_finite())
        }
    });
    let Some(index) = index else {
        return;
    };
    if let Some(vertical) = stroke_path(
        &[point(cursor.x, top), point(cursor.x, bottom)],
        opts.stroke_width.max(1.0),
    ) {
        window.paint_path(vertical, Rgba { a: 0.48, ..base });
    }
    let series_mark = |window: &mut Window, series: &[f32], color: Rgba| {
        let Some(&value) = series.get(index).filter(|value| value.is_finite()) else {
            return;
        };
        let y_value = (value / opts.max.max(1e-6)).clamp(0.0, 1.0);
        let sample_point = point(
            if slide {
                sample_x_slide(left, width, index, opts.data_points, slide_progress)
            } else {
                sample_x(left, width, index, series.len(), opts.data_points)
            },
            bottom - height * y_value,
        );
        if let Some(horizontal) = stroke_path(
            &[point(left, sample_point.y), point(right, sample_point.y)],
            opts.stroke_width.max(1.0),
        ) {
            window.paint_path(horizontal, Rgba { a: 0.48, ..color });
        }
        let marker = Bounds {
            origin: point(sample_point.x - px(3.0), sample_point.y - px(3.0)),
            size: size(px(6.0), px(6.0)),
        };
        window.paint_quad(fill(marker, Hsla::from(color)));
    };
    // Secondary first so the family-solid series' dot sits on top.
    if let Some((secondary_samples, secondary_base)) = secondary {
        series_mark(window, secondary_samples, secondary_base);
    }
    series_mark(window, samples, base);
}

/// The shared inputs of a stateful hover graph element: its element identity,
/// the series ink, the rendering options, the tooltip formatter, and the
/// window-owned hover slot + scene cache the element reads and writes. The
/// single-series and two-series constructors differ only in the optional
/// [`GraphHoverDual`] direction pair.
pub(crate) struct GraphHoverElement<F> {
    /// Element id, also the `tm-graph:{id}` debug-selector base.
    pub(crate) id: ElementId,
    /// Element id whose slide timing drives the refresh animation.
    pub(crate) slide_key: ElementId,
    /// Window-limited primary sample series (one `Rc` clone per UI-only frame).
    pub(crate) samples: Rc<[f32]>,
    /// Primary (family) color.
    pub(crate) base: Rgba,
    /// Geometry, scale, and animation tunables.
    pub(crate) opts: GraphOpts,
    /// Tooltip value formatter for the hovered slot.
    pub(crate) format_value: F,
    /// Window-owned hover slot the pointer handlers write and the tooltip reads.
    pub(crate) slot: Rc<RefCell<Option<GraphHover>>>,
    /// Cross-frame scene cache shared by every graph on the page.
    pub(crate) cache: GraphCacheHandle,
}

/// The optional second direction of a two-series hover graph: the primary
/// direction's tooltip label plus the [`GraphSecondarySeries`] ink (its
/// samples, tint, and direction label).
pub(crate) struct GraphHoverDual {
    /// Label prefixing the primary direction in the composed tooltip.
    pub(crate) primary_label: String,
    /// The secondary series' samples, tint, and label.
    pub(crate) secondary: GraphSecondarySeries,
}

/// Stateful graph canvas with per-window tooltip and crosshair projection.
///
/// `samples` is a shared `Rc<[f32]>`. Callers whose projection is already
/// generation-cached (per-core grid, memory page) pass the `Rc` so a UI-only
/// frame (hover, resize, animation repaint) pays one `Rc` clone instead of
/// copying the whole history window into the element.
pub(crate) fn graph_element_hover<F>(inputs: GraphHoverElement<F>) -> AnyElement
where
    F: Fn(f32) -> String + 'static,
{
    graph_element_hover_impl(inputs, None)
}

/// The two-series variant: the primary series wears the family token, the
/// secondary (see [`GraphSecondarySeries`]) the same token lifted toward
/// white, both through one shared slot grid and one shared `opts.max`.
/// `dual.primary_label` prefixes the primary direction in the composed tooltip.
pub(crate) fn graph_element_hover_dual<F>(
    inputs: GraphHoverElement<F>,
    dual: GraphHoverDual,
) -> AnyElement
where
    F: Fn(f32) -> String + 'static,
{
    graph_element_hover_impl(inputs, Some(dual))
}

fn graph_element_hover_impl<F>(
    inputs: GraphHoverElement<F>,
    dual: Option<GraphHoverDual>,
) -> AnyElement
where
    F: Fn(f32) -> String + 'static,
{
    let GraphHoverElement {
        id,
        slide_key,
        samples,
        base,
        opts,
        format_value,
        slot,
        cache,
    } = inputs;
    let debug_selector = format!("tm-graph:{id}");
    let bounds = Rc::new(RefCell::new(None::<Bounds<Pixels>>));
    // Window limiting keeps the series' identity on UI-only frames. A
    // two-series graph slides only when BOTH directions carry the extra older
    // sample; otherwise both fall back to the exact settled window so the two
    // curves never disagree about the x mapping (the primary must not spread
    // a capacity+1 window while the secondary anchors capacity).
    let primary_slide_supported = graph_slide_supported(&samples, opts.data_points);
    let slide_supported = primary_slide_supported
        && dual
            .as_ref()
            .is_none_or(|dual| graph_slide_supported(&dual.secondary.samples, opts.data_points));
    let (samples_rc, secondary_rc) = match (&dual, opts.sliding) {
        (_, false) => {
            let primary = cache
                .borrow_mut()
                .latest_samples(samples, opts.data_points, false);
            let secondary = dual.as_ref().map(|dual| {
                cache.borrow_mut().latest_samples(
                    Rc::clone(&dual.secondary.samples),
                    opts.data_points,
                    false,
                )
            });
            (primary, secondary)
        }
        (None, true) => {
            let primary = cache
                .borrow_mut()
                .latest_samples(samples, opts.data_points, true);
            (primary, None)
        }
        (Some(dual), true) => {
            let primary =
                cache
                    .borrow_mut()
                    .latest_samples(Rc::clone(&samples), opts.data_points, true);
            let secondary_samples = cache.borrow_mut().latest_samples(
                Rc::clone(&dual.secondary.samples),
                opts.data_points,
                true,
            );
            if slide_supported {
                (primary, Some(secondary_samples))
            } else {
                let primary = cache
                    .borrow_mut()
                    .latest_samples(primary, opts.data_points, false);
                let secondary =
                    cache
                        .borrow_mut()
                        .latest_samples(secondary_samples, opts.data_points, false);
                (primary, Some(secondary))
            }
        }
    };
    let sliding = opts.sliding && slide_supported;
    let paint_bounds = bounds.clone();
    let paint_samples = samples_rc.clone();
    let paint_secondary = secondary_rc.clone();
    let paint_secondary_base = dual.as_ref().map(|dual| dual.secondary.base);
    // The direction labels ride along so the value badge can compose both
    // directions' newest values (the legend names the same pairing above the
    // card); cloned Strings because the paint closure outlives `dual`.
    let paint_dual_labels = dual
        .as_ref()
        .map(|dual| (dual.primary_label.clone(), dual.secondary.label.clone()));
    let paint_hover_slot = slot.clone();
    let paint_cache = cache.clone();
    let graph_canvas = canvas(
        |_bounds, _window, _cx| (),
        move |bnd, _t, window, cx| {
            *paint_bounds.borrow_mut() = Some(bnd);
            let mut cache = paint_cache.borrow_mut();
            let slide_started_at = sliding.then(|| {
                cache
                    .slides_mut()
                    .timing_for(&slide_key, &paint_samples, Instant::now())
            });
            let progress =
                slide_started_at.map_or(1.0, |started_at| slide_progress(started_at, window));
            let offset = if sliding {
                let slot = graph_slide_spacing(bnd, opts.data_points);
                px(-f32::from(slot) * progress)
            } else {
                px(0.0)
            };
            // Rebuild only when samples/bounds/opts changed; hover-only
            // repaints replay the cached geometry (see `scene_cache`).
            match (&paint_secondary, paint_secondary_base) {
                (Some(secondary), Some(secondary_base)) => {
                    let labels = paint_dual_labels.as_ref();
                    paint_graph_dual_scene(
                        GraphPaintContext {
                            cache: cache.scenes_mut(),
                            window: &mut *window,
                            cx: &mut *cx,
                            bounds: bnd,
                            opts,
                            slide_offset: offset,
                        },
                        &DualGraphSeries {
                            samples: &paint_samples,
                            base,
                            label: labels.map(|(primary_label, _)| primary_label.as_str()),
                        },
                        &DualGraphSeries {
                            samples: secondary,
                            base: secondary_base,
                            label: labels.map(|(_, secondary_label)| secondary_label.as_str()),
                        },
                    );
                }
                _ => paint_graph_scene(
                    GraphPaintContext {
                        cache: cache.scenes_mut(),
                        window: &mut *window,
                        cx: &mut *cx,
                        bounds: bnd,
                        opts,
                        slide_offset: offset,
                    },
                    &paint_samples,
                    base,
                ),
            }
            let cursor = paint_hover_slot.borrow().as_ref().map(|hover| hover.cursor);
            draw_graph_crosshair(
                window,
                GraphCrosshair {
                    bounds: bnd,
                    samples: paint_samples.as_ref(),
                    base,
                    opts,
                    cursor,
                    slide: sliding.then_some(progress),
                    secondary: paint_secondary_base
                        .zip(paint_secondary.as_deref())
                        .map(|(secondary_base, secondary)| (secondary, secondary_base)),
                },
            );
        },
    )
    .size_full();

    let mv_bounds = bounds.clone();
    let mv_samples = samples_rc.clone();
    let mv_secondary = dual.as_ref().map(|dual| {
        (
            dual.primary_label.clone(),
            secondary_rc.clone(),
            dual.secondary.label.clone(),
        )
    });
    let mv_fmt = format_value;
    let mv_slot = slot.clone();
    let move_cache = cache.clone();
    let mv_index = Rc::new(Cell::new(None::<usize>));
    let move_index = mv_index.clone();
    let move_handler = move |ev: &MouseMoveEvent, win: &mut Window, _cx: &mut App| {
        let pos = ev.position;
        let index = {
            let bnd = mv_bounds.borrow();
            bnd.as_ref().and_then(|bounds| {
                sample_index_at_cursor_x(
                    mv_samples.as_ref(),
                    bounds.origin.x,
                    bounds.size.width,
                    pos.x,
                    opts.data_points,
                )
                .or_else(|| {
                    // One direction may hold the only evidence at this slot;
                    // resolve the slot positionally and let the text composer
                    // decide which series can speak.
                    let (_, secondary, _) = mv_secondary.as_ref()?;
                    let secondary = secondary.as_deref()?;
                    sample_slot_at_cursor_x(
                        secondary,
                        bounds.origin.x,
                        bounds.size.width,
                        pos.x,
                        opts.data_points,
                    )
                })
            })
        };
        // `None` is both the initial cache state and the result for an empty
        // or provider-gap history. Only treat it as a stable slot when the
        // visible hover is already clear; otherwise the first move after a
        // programmatic/test hover must still clear the slot.
        let same_slot = match (move_index.get(), index, mv_slot.borrow().is_some()) {
            (Some(previous), Some(next), true) => previous == next,
            (None, None, false) => true,
            _ => false,
        };
        if same_slot {
            let should_refresh = mv_slot.borrow().as_ref().is_some_and(|previous| {
                (f32::from(previous.cursor.x) - f32::from(pos.x)).abs() >= 1.0
                    || (f32::from(previous.cursor.y) - f32::from(pos.y)).abs() >= 1.0
            });
            if !should_refresh {
                return;
            }
            // Rate gate (see `scene_cache`): gpui 0.2.2 repaints the whole
            // window per refresh and pointer events can outpace any display,
            // so crosshair-follow refreshes are capped. The slot is only
            // updated when the refresh goes through — a gated-out event
            // leaves the painted state and the slot consistent, so the
            // crosshair trails the pointer by at most one gate interval
            // instead of freezing at a stale position.
            if !move_cache.borrow_mut().hover_refresh_due(Instant::now()) {
                return;
            }
            if let Some(hover) = mv_slot.borrow_mut().as_mut() {
                hover.cursor = pos;
            }
            win.refresh();
        } else {
            // A value change is equally gated: the tooltip content would
            // otherwise mutate without a repaint, desynchronizing the painted
            // tooltip from the slot until the next unrelated refresh.
            if !move_cache.borrow_mut().hover_refresh_due(Instant::now()) {
                return;
            }
            move_index.set(index);
            let text = index.and_then(|index| match &mv_secondary {
                None => mv_samples
                    .get(index)
                    .copied()
                    .filter(|value| value.is_finite())
                    .map(&mv_fmt),
                Some((primary_label, secondary_samples, secondary_label)) => {
                    multi_series_hover_text(
                        primary_label,
                        mv_samples.as_ref(),
                        Some((
                            secondary_label.as_str(),
                            secondary_samples.as_deref().unwrap_or(&[]),
                        )),
                        index,
                        &mv_fmt,
                    )
                }
            });
            *mv_slot.borrow_mut() = text.map(|text| GraphHover { cursor: pos, text });
            win.refresh();
        }
    };

    let hov_slot = slot.clone();
    let hov_index = mv_index;
    let hover_cache = cache;
    let hover_handler = move |is_hov: &bool, win: &mut Window, _cx: &mut App| {
        if !is_hov {
            hov_index.set(None);
            *hov_slot.borrow_mut() = None;
            // Leaving the graph must always repaint immediately (clear the
            // crosshair/tooltip) and re-arm the gate for the next enter.
            hover_cache.borrow_mut().reset_hover_refresh();
            win.refresh();
        }
    };

    let interactive = div()
        .id(id)
        .debug_selector(move || debug_selector)
        .size_full()
        .on_mouse_move(move_handler)
        .on_hover(hover_handler)
        .child(graph_canvas);
    div()
        .size_full()
        .overflow_hidden()
        .child(interactive)
        .into_any_element()
}
