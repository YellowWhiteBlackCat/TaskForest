//! Custom-drawn scrollbar (absorption §7.3-7.6): geometry math and the
//! 3-second fade state machine are pure functions with headless tests; the
//! element wires them to mouse/wheel events and paints with gpui canvas.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    App, BorderStyle, Bounds, DispatchPhase, Edges, Element, ElementId, Entity, GlobalElementId,
    HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, Position, RenderOnce, ScrollHandle,
    Size, Style, Window, point, px, relative, size,
};

use taskmanager_theme::{Palette, with_alpha};

mod geometry;
pub mod rail;

pub use geometry::{
    FADE_OUT_DELAY, FADE_OUT_DURATION, MIN_THUMB_SIZE, SCROLLBAR_HEIGHT, SCROLLBAR_WIDTH,
    THUMB_ACTIVE_WIDTH, THUMB_INSET, THUMB_WIDTH, ThumbGeometry, drag_percentage, fade_opacity,
    is_scrollbar_visible, thumb_geometry, thumb_geometry_for_track, track_click_offset,
    track_click_percentage,
};

/// Scrollbar show mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollbarShow {
    /// Fade out 2s after the last scroll/hover.
    #[default]
    Scrolling,
    /// Only while hovering the scrollbar area.
    Hover,
    /// Always visible.
    Always,
}

/// Abstraction over anything that can scroll (absorbing gc's trait).
pub trait ScrollbarHandle: 'static {
    /// Current scroll offset.
    fn offset(&self) -> Point<Pixels>;
    /// Set the scroll offset.
    fn set_offset(&self, offset: Point<Pixels>);
    /// Maximum positive scroll range. GPUI stores the visible offset as a
    /// negative point, while `max_offset` is the corresponding positive
    /// range. Keeping this as a first-class input avoids reconstructing the
    /// range from a possibly stale content measurement.
    fn max_offset(&self) -> Size<Pixels>;
    /// The actual viewport tracked by the scrolling element.
    fn viewport(&self) -> Bounds<Pixels>;
    /// Full content size, retained as a convenience for custom handles.
    fn content_size(&self) -> Size<Pixels> {
        self.viewport().size + self.max_offset()
    }
    /// Dragging starts (hosts may pause auto-scroll).
    fn start_drag(&self) {}
    /// Dragging ends.
    fn end_drag(&self) {}
}

impl ScrollbarHandle for ScrollHandle {
    fn offset(&self) -> Point<Pixels> {
        self.offset()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.set_offset(offset);
    }

    fn max_offset(&self) -> Size<Pixels> {
        self.max_offset()
    }

    fn viewport(&self) -> Bounds<Pixels> {
        self.bounds()
    }
}

/// Element-local state for one scrollbar (Copy + `Rc<Cell>` pattern per
/// absorption 7.6-4: multi-closure shared writes go through `set(get().with_…)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarInnerState {
    pub dragged: bool,
    /// Pointer is over the track strip (hover affordance).
    pub hovering: bool,
    pub grab_offset: f32,
    pub last_scroll: Option<Instant>,
    /// Last offset the wheel handler observed (to detect scroll changes).
    pub last_offset: Point<Pixels>,
    /// Last time a drag refresh was allowed through to the window.
    pub last_refresh: Option<Instant>,
    /// A drag moved the handle during the current refresh interval.  The
    /// next animation frame clears this flag; pointer events then coalesce
    /// behind one scheduled frame instead of queueing one refresh per event.
    pub refresh_pending: bool,
}

impl Default for ScrollbarInnerState {
    fn default() -> Self {
        Self {
            dragged: false,
            hovering: false,
            grab_offset: 0.0,
            last_scroll: None,
            last_offset: point(px(0.0), px(0.0)),
            last_refresh: None,
            refresh_pending: false,
        }
    }
}

impl ScrollbarInnerState {
    /// Pure transition: record a scroll event.
    pub fn with_scroll(mut self, offset: Point<Pixels>, now: Instant) -> Self {
        self.last_offset = offset;
        self.last_scroll = Some(now);
        self
    }

    /// Mark the start of a paint.  A pending drag refresh has now been
    /// delivered, so a later pointer event may schedule another one.
    pub fn frame_started(mut self) -> Self {
        self.refresh_pending = false;
        self
    }

    /// Decide whether a drag may refresh immediately.  The offset itself is
    /// updated on every pointer event; only the expensive tree rebuild is
    /// rate-limited.  This prevents a final pointer position from being lost
    /// when the pointer stops inside the throttle interval.
    pub fn allow_drag_refresh(&mut self, now: Instant, max_fps: usize) -> bool {
        let interval = Duration::from_millis((1000 / max_fps.max(1)) as u64);
        if self
            .last_refresh
            .map(|last| now.saturating_duration_since(last) >= interval)
            .unwrap_or(true)
        {
            self.last_refresh = Some(now);
            self.refresh_pending = false;
            true
        } else {
            self.refresh_pending = true;
            false
        }
    }
}

/// Which axis the scrollbar tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarAxis {
    /// Right-edge vertical scrollbar.
    Vertical,
    /// Bottom-edge horizontal scrollbar.
    Horizontal,
}

/// The scrollbar component: a full-size hit strip; the thumb is painted at
/// the axis edge. `ScrollbarHandle` is the offset source of truth.
///
/// The component owns its interaction state in a GPUI entity rather than in a
/// per-element `Rc<Cell<_>>`. GPUI 0.2.2 still rebuilds the root element tree
/// when a view is invalidated, but this keeps drag state independent from that
/// rebuild and lets the refresh gate invalidate only once per display frame.
#[derive(IntoElement)]
pub struct Scrollbar {
    id: ElementId,
    axis: ScrollbarAxis,
    handle: Rc<dyn ScrollbarHandle>,
    show: ScrollbarShow,
    palette: Palette,
    max_fps: usize,
    track_inset: Pixels,
}

#[derive(Default)]
struct ScrollbarState {
    inner: ScrollbarInnerState,
}

struct ScrollbarElement {
    scrollbar: Scrollbar,
    state: Entity<ScrollbarState>,
}

impl Scrollbar {
    /// Build a vertical scrollbar for `handle`.
    pub fn vertical(
        id: impl Into<ElementId>,
        handle: Rc<dyn ScrollbarHandle>,
        palette: Palette,
    ) -> Self {
        Self {
            id: id.into(),
            axis: ScrollbarAxis::Vertical,
            handle,
            // A vertical rail is a persistent navigation affordance.  The
            // thumb still disappears when there is no overflow, but it must
            // not fade away while the page remains scrollable; doing so makes
            // dense tables and device lists look clipped and undiscoverable.
            show: ScrollbarShow::Always,
            palette,
            max_fps: 60,
            track_inset: px(0.0),
        }
    }

    /// Build a horizontal scrollbar for `handle`.
    pub fn horizontal(
        id: impl Into<ElementId>,
        handle: Rc<dyn ScrollbarHandle>,
        palette: Palette,
    ) -> Self {
        Self {
            id: id.into(),
            axis: ScrollbarAxis::Horizontal,
            handle,
            show: ScrollbarShow::Scrolling,
            palette,
            max_fps: 60,
            track_inset: px(0.0),
        }
    }

    /// Override the show mode.
    #[must_use]
    pub fn show(mut self, show: ScrollbarShow) -> Self {
        self.show = show;
        self
    }

    /// Drag update cap (frames per second), default 60.
    #[must_use]
    pub fn max_fps(mut self, max_fps: usize) -> Self {
        self.max_fps = max_fps.max(1);
        self
    }

    /// Inset the visual and interactive track while keeping the outer hit
    /// strip full-size. Rails use this to align the thumb with their hairline
    /// track; direct scrollbars can keep the default zero inset.
    #[must_use]
    pub fn track_inset(mut self, inset: Pixels) -> Self {
        self.track_inset = inset.max(px(0.0));
        self
    }

    /// Fill-quad helper (bounds + color + corner radii).
    fn fill(bounds: Bounds<Pixels>, color: Hsla) -> PaintQuad {
        PaintQuad {
            bounds,
            corner_radii: (0.0).into(),
            background: color.into(),
            border_widths: Edges::default(),
            border_color: Hsla::transparent_black(),
            border_style: BorderStyle::default(),
        }
    }
}

impl RenderOnce for Scrollbar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_window, _cx| {
            ScrollbarState::default()
        });
        ScrollbarElement {
            scrollbar: self,
            state,
        }
    }
}

impl IntoElement for ScrollbarElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

fn refresh_window(window: &mut Window) {
    window.refresh();
}

impl Element for ScrollbarElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.scrollbar.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            position: Position::Absolute,
            // An absolute child without explicit insets can retain its flex
            // static position. In a rail that puts the painted thumb to the
            // left of the hairline track even though both children share the
            // same wrapper. Pin the scroll element to the wrapper so its
            // cross-axis center is the rail's center, not the previous flex
            // sibling's static position.
            inset: Edges::all(px(0.0).into()),
            flex_grow: 1.0,
            flex_shrink: 1.0,
            size: Size {
                width: relative(1.0).into(),
                height: relative(1.0).into(),
            },
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        window.insert_hitbox(bounds, HitboxBehavior::Normal);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Persistent per-scrollbar interaction state (drag grab, fade clock,
        // hover) survives root element reconstruction. A per-paint
        // `Rc::new(Cell::new(default))` resets every frame and makes a drag
        // lose its capture state as soon as the window is refreshed.
        let state = self.state.clone();
        state.update(cx, |state, _cx| {
            state.inner = state.inner.frame_started();
        });

        let palette = self.scrollbar.palette;
        let is_vertical = self.scrollbar.axis == ScrollbarAxis::Vertical;
        let requested_hit_thickness = if is_vertical {
            SCROLLBAR_WIDTH
        } else {
            SCROLLBAR_HEIGHT
        };

        // The rail is an overlay, but the scroll handle remains the geometry
        // authority. Use its actual viewport and max range rather than
        // reconstructing content size from the rail's own bounds. This keeps
        // the thumb aligned when padding, flex growth, or a compact resize
        // changes the viewport without changing the rail hit strip.
        let viewport = self.scrollbar.handle.viewport();
        let max_offset = self.scrollbar.handle.max_offset();
        let container = if is_vertical {
            f32::from(viewport.size.height)
        } else {
            f32::from(viewport.size.width)
        };
        let scroll_range = if is_vertical {
            f32::from(max_offset.height).max(0.0)
        } else {
            f32::from(max_offset.width).max(0.0)
        };
        let axis_length = if is_vertical {
            f32::from(bounds.size.height)
        } else {
            f32::from(bounds.size.width)
        };
        let track_inset = f32::from(self.scrollbar.track_inset)
            .max(0.0)
            .min((axis_length / 2.0).max(0.0));
        let track_length = (axis_length - track_inset * 2.0).max(0.0);
        let hit_thickness = requested_hit_thickness.min(if is_vertical {
            f32::from(bounds.size.width)
        } else {
            f32::from(bounds.size.height)
        });
        let offset = self.scrollbar.handle.offset();
        let scroll_offset = if is_vertical {
            f32::from(offset.y)
        } else {
            f32::from(offset.x)
        };
        let geometry = (container > 0.0 && track_length > 0.0 && scroll_range > 0.0).then(|| {
            thumb_geometry_for_track(
                container + scroll_range,
                container,
                track_length,
                -scroll_range.min((-scroll_offset).max(0.0)),
            )
        });

        // Track bounds: a narrow hit strip at the axis edge. Its axis is
        // inset with the visual rail, while the surrounding element still
        // owns the full-size hitbox for capture-phase drag release.
        let track_bounds = if is_vertical {
            Bounds {
                origin: point(bounds.right() - px(hit_thickness), bounds.top()),
                size: size(px(hit_thickness), px(axis_length)),
            }
        } else {
            Bounds {
                origin: point(bounds.left(), bounds.bottom() - px(hit_thickness)),
                size: size(px(axis_length), px(hit_thickness)),
            }
        };
        let track_bounds = if is_vertical {
            Bounds {
                origin: point(track_bounds.left(), track_bounds.top() + px(track_inset)),
                size: size(track_bounds.size.width, px(track_length)),
            }
        } else {
            Bounds {
                origin: point(track_bounds.left() + px(track_inset), track_bounds.top()),
                size: size(px(track_length), track_bounds.size.height),
            }
        };

        // Keep the fade clock fresh when the offset changed since the last
        // observed value (wheel scrolling is handled by the scroll container
        // itself; this paint already sees the new offset, so no refresh is
        // needed here).
        let current_offset = self.scrollbar.handle.offset();
        let inner = state.read(cx).inner;
        if current_offset != inner.last_offset {
            let next = inner.with_scroll(current_offset, Instant::now());
            state.update(cx, |state, _cx| state.inner = next);
        }

        let inner = state.read(cx).inner;
        let elapsed = inner.last_scroll.map(|t| t.elapsed().as_secs_f32());
        let overflow = geometry.is_some();
        let visible = overflow
            && is_scrollbar_visible(inner.dragged, inner.hovering, elapsed, self.scrollbar.show);

        if visible && let Some(geometry) = geometry {
            // `Always` ignores the fade clock (a pinned rail); the interactive
            // modes hold full opacity while dragged/hovered and fade otherwise.
            let opacity = if self.scrollbar.show == ScrollbarShow::Always
                || inner.dragged
                || inner.hovering
            {
                1.0
            } else {
                elapsed.map(fade_opacity).unwrap_or(0.0)
            };

            // Trackless overlay style (macOS/Adwaita/Win11 overlay scrollers):
            // no idle track wash — hosts that want a visible rail paint their
            // own behind the element (the Settings dialog does). The thumb is
            // a capsule that thickens while active (hover or drag).
            let active = inner.dragged || inner.hovering;
            let thumb_width = if active {
                THUMB_ACTIVE_WIDTH
            } else {
                THUMB_WIDTH
            };
            let inset = THUMB_INSET.min(geometry.thumb_length / 2.0);
            let thumb_length = (geometry.thumb_length - inset * 2.0)
                .max(1.0)
                .min(geometry.thumb_length);

            // Thumb rect. The hit geometry and paint geometry are separate:
            // the full outer thumb is draggable while only the inner capsule
            // is painted, so the 3px rail remains easy to grab.
            let thumb = if is_vertical {
                Bounds {
                    origin: point(
                        track_bounds.right() - px(hit_thickness / 2.0 + thumb_width / 2.0),
                        track_bounds.top() + px(geometry.thumb_start + inset),
                    ),
                    size: size(px(thumb_width), px(thumb_length)),
                }
            } else {
                Bounds {
                    origin: point(
                        track_bounds.left() + px(geometry.thumb_start + inset),
                        track_bounds.bottom() - px(hit_thickness / 2.0 + thumb_width / 2.0),
                    ),
                    size: size(px(thumb_length), px(thumb_width)),
                }
            };

            if opacity > 0.0 {
                // Thumb: a neutral foreground-derived tone, NOT the saturated
                // selection `accent` (Win11 TM / libadwaita / macOS all reserve
                // accent for selection and paint the knob as a muted fg). `fg`
                // is guaranteed max-contrast in every skin, so 30%/55% read on
                // any background; active states brighten instead of recoloring.
                let thumb_alpha = if active { 0.55 } else { 0.30 };
                window.paint_quad(
                    Scrollbar::fill(
                        thumb,
                        crate::theme_binding::hsla(with_alpha(palette.fg, thumb_alpha * opacity)),
                    )
                    .corner_radii(px(thumb_width / 2.0)),
                );
            }

            // While the fade window is open the thumb still changes opacity:
            // keep frames coming until it is fully gone, then stop (a bounded
            // animation, not a refresh loop). `Always` never animates.
            if self.scrollbar.show != ScrollbarShow::Always
                && elapsed.is_some_and(|e| e < FADE_OUT_DURATION)
            {
                window.request_animation_frame();
            }
        }

        // Mouse wiring.
        let state_entity_state = state.clone();
        let handle = self.scrollbar.handle.clone();
        let max_fps = self.scrollbar.max_fps;
        let is_vertical_drag = is_vertical;
        let thumb_bounds = geometry.map(|geometry| {
            if is_vertical {
                Bounds {
                    origin: point(
                        track_bounds.left(),
                        track_bounds.top() + px(geometry.thumb_start),
                    ),
                    size: size(track_bounds.size.width, px(geometry.thumb_length)),
                }
            } else {
                Bounds {
                    origin: point(
                        track_bounds.left() + px(geometry.thumb_start),
                        track_bounds.top(),
                    ),
                    size: size(px(geometry.thumb_length), track_bounds.size.height),
                }
            }
        });

        // Mouse down on track/thumb.
        window.on_mouse_event({
            let state = state_entity_state.clone();
            let handle = handle.clone();
            move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || event.button != MouseButton::Left
                    || !track_bounds.contains(&event.position)
                {
                    return;
                }
                let Some(thumb_bounds) = thumb_bounds else {
                    return;
                };
                cx.stop_propagation();
                let pointer = if is_vertical_drag {
                    f32::from(event.position.y)
                } else {
                    f32::from(event.position.x)
                };
                let thumb_start = if is_vertical_drag {
                    f32::from(thumb_bounds.top())
                } else {
                    f32::from(thumb_bounds.left())
                };
                let thumb_end = if is_vertical_drag {
                    f32::from(thumb_bounds.bottom())
                } else {
                    f32::from(thumb_bounds.right())
                };
                let max_offset = handle.max_offset();
                let range = if is_vertical_drag {
                    f32::from(max_offset.height).max(0.0)
                } else {
                    f32::from(max_offset.width).max(0.0)
                };

                if pointer >= thumb_start && pointer <= thumb_end {
                    let grab = pointer - thumb_start;
                    let mut next = state.read(cx).inner;
                    next.dragged = true;
                    next.grab_offset = grab;
                    next.last_scroll = Some(Instant::now());
                    next.last_offset = handle.offset();
                    next.last_refresh = None;
                    next.refresh_pending = false;
                    state.update(cx, |state, _cx| state.inner = next);
                    handle.start_drag();
                } else {
                    let percentage = track_click_percentage(
                        pointer,
                        if is_vertical_drag {
                            f32::from(track_bounds.top())
                        } else {
                            f32::from(track_bounds.left())
                        },
                        if is_vertical_drag {
                            f32::from(track_bounds.size.height)
                        } else {
                            f32::from(track_bounds.size.width)
                        },
                        if is_vertical_drag {
                            f32::from(thumb_bounds.size.height)
                        } else {
                            f32::from(thumb_bounds.size.width)
                        },
                    );
                    let jump = px(-range * percentage);
                    let offset = handle.offset();
                    let new_offset = if is_vertical_drag {
                        point(offset.x, jump)
                    } else {
                        point(jump, offset.y)
                    };
                    handle.set_offset(new_offset);
                    let next = state.read(cx).inner.with_scroll(new_offset, Instant::now());
                    state.update(cx, |state, _cx| state.inner = next);
                }
                refresh_window(window);
            }
        });

        // Mouse move: drag follows the pointer with the kept grab offset;
        // otherwise hover enter/leave toggles the thumb affordance and
        // restarts the fade clock so an exited hover fades out smoothly.
        window.on_mouse_event({
            let state = state_entity_state.clone();
            let handle = handle.clone();
            move |event: &MouseMoveEvent, phase, window, cx| {
                let mut inner = state.read(cx).inner;
                if inner.dragged {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    cx.stop_propagation();
                    if !event.dragging() {
                        return;
                    }
                    let Some(thumb_bounds) = thumb_bounds else {
                        return;
                    };
                    let pointer = if is_vertical_drag {
                        f32::from(event.position.y)
                    } else {
                        f32::from(event.position.x)
                    };
                    let track_start = if is_vertical_drag {
                        f32::from(track_bounds.top())
                    } else {
                        f32::from(track_bounds.left())
                    };
                    let track_len = if is_vertical_drag {
                        f32::from(track_bounds.size.height)
                    } else {
                        f32::from(track_bounds.size.width)
                    };
                    let thumb_len = if is_vertical_drag {
                        f32::from(thumb_bounds.size.height)
                    } else {
                        f32::from(thumb_bounds.size.width)
                    };
                    let percentage = drag_percentage(
                        pointer,
                        inner.grab_offset,
                        track_start,
                        track_len,
                        thumb_len,
                    );
                    let max_offset = handle.max_offset();
                    let range = if is_vertical_drag {
                        f32::from(max_offset.height).max(0.0)
                    } else {
                        f32::from(max_offset.width).max(0.0)
                    };
                    let current = handle.offset();
                    let new_offset = if is_vertical_drag {
                        point(current.x, px(-range * percentage))
                    } else {
                        point(px(-range * percentage), current.y)
                    };
                    let now = Instant::now();
                    if handle.offset() != new_offset {
                        handle.set_offset(new_offset);
                        inner = inner.with_scroll(new_offset, now);
                        let allow_refresh = inner.allow_drag_refresh(now, max_fps);
                        state.update(cx, |state, _cx| state.inner = inner);
                        // The first allowed refresh dirties the window. While
                        // that frame is pending, more pointer events only
                        // replace the offset; `request_animation_frame` is a
                        // paint-only API in GPUI 0.2.2 and is intentionally
                        // not called from this event callback.
                        if allow_refresh {
                            refresh_window(window);
                        }
                    }
                    return;
                }
                if phase != DispatchPhase::Capture {
                    return;
                }
                let hovering_now = track_bounds.contains(&event.position);
                if inner.hovering != hovering_now {
                    inner.hovering = hovering_now;
                    inner.last_scroll = Some(Instant::now());
                    state.update(cx, |state, _cx| state.inner = inner);
                    refresh_window(window);
                }
            }
        });

        // Mouse up: end drag.
        window.on_mouse_event({
            let state = state_entity_state.clone();
            let handle = handle.clone();
            move |_event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || !state.read(cx).inner.dragged {
                    return;
                }
                let mut inner = state.read(cx).inner;
                inner.dragged = false;
                state.update(cx, |state, _cx| state.inner = inner);
                handle.end_drag();
                refresh_window(window);
            }
        });
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_scrollbar_tests.rs"]
mod tests;
