//! Win11-TM-style segmented control: one connected rounded track of flush
//! segments (the Processes view-mode switcher + the status-filter row). The
//! active segment fills with the accent (`on_accent` text); idle segments are
//! transparent over the track surface and pick up `palette.hover` on hover; 1px
//! hairline dividers separate adjacent segments.
//!
//! Keyboard (WCAG 2.4.7 / ARIA radiogroup): the whole track is ONE tab stop.
//! Left / Right moves the selection to the adjacent segment (wrapping) and
//! fires that segment's `on_click`, exactly as if it were clicked — so keyboard
//! users traverse the control in a single Tab instead of N, and there are no
//! per-segment tab stops to manage. The focus-visible ring is a 2px accent
//! outset `BoxShadow` at `palette.ring` scaled to 0.6 alpha — byte-identical to
//! the app-layer `elements::focus_ring_shadow` (same accent hue, same 0.6
//! alpha, same 2px outset, same keyboard-only gating via `palette.ring.a`),
//! which cannot be imported here without an app↔ui cycle; `button.rs` follows
//! the same `palette.ring` idiom.
//!
//! Call sites bind their `Entity<RootView>` updates through the same closure
//! shape `elements::pill` consumes (`Fn(&mut Window, &mut App)` /
//! `Fn(&bool, &mut Window, &mut App)`), so the primitive stays decoupled from
//! any particular view.

use std::rc::Rc;

use gpui::{
    App, BoxShadow, ElementId, InteractiveElement, IntoElement, ParentElement, Point, RenderOnce,
    Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use taskmanager_theme::Palette;
use taskmanager_theme::tokens;

use crate::styled::on_accent;

type ClickHandler = dyn Fn(&mut Window, &mut App);
type HoverHandler = dyn Fn(&bool, &mut Window, &mut App);

/// Resolved per-segment fill family (palette-derived, no hardcoded hues).
/// `active` wins over `hovered` — a selected segment stays accent-filled even
/// while also hovered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SegmentFill {
    /// Accent fill + max-contrast `on_accent` text.
    Active,
    /// `palette.hover` tint over the track surface + foreground text.
    Hovered,
    /// Transparent (track surface shows) + foreground text.
    Idle,
}

fn fill_for(active: bool, hovered: bool) -> SegmentFill {
    if active {
        SegmentFill::Active
    } else if hovered {
        SegmentFill::Hovered
    } else {
        SegmentFill::Idle
    }
}

/// One segment of a [`Segmented`] control.
///
/// Built via [`Segment::new`] + the `active` / `hovered` setters. The click and
/// hover closures mirror the shape the rest of the chrome consumes — the owning
/// view publishes its hover slot through `on_hover` (the same single-slot
/// `Hover::Static(id)` pattern the chrome pills use) and mutates its selection
/// through `on_click`.
#[derive(Clone)]
pub struct Segment {
    id: ElementId,
    label: SharedString,
    active: bool,
    hovered: bool,
    on_click: Rc<ClickHandler>,
    on_hover: Rc<HoverHandler>,
}

impl Segment {
    /// Create a segment with its click/hover closures bound. Defaults: inactive,
    /// not hovered.
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        on_click: impl Fn(&mut Window, &mut App) + 'static,
        on_hover: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            active: false,
            hovered: false,
            on_click: Rc::new(on_click),
            on_hover: Rc::new(on_hover),
        }
    }

    /// Mark this as the currently-selected segment (accent fill + `on_accent`).
    #[must_use]
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Hovered idle segment → `palette.hover` tint. The caller owns the bool and
    /// publishes it via `on_hover`.
    #[must_use]
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }
}

/// A connected segmented control. See the [module docs](self) for the keyboard
/// contract and the visual spec.
#[derive(IntoElement)]
pub struct Segmented {
    id: ElementId,
    segments: Vec<Segment>,
    palette: Palette,
}

impl Segmented {
    /// Create an empty track; add segments via [`Segmented::segment`].
    pub fn new(id: impl Into<ElementId>, palette: Palette) -> Self {
        Self {
            id: id.into(),
            segments: Vec::new(),
            palette,
        }
    }

    /// Append a segment (chainable).
    #[must_use]
    pub fn segment(mut self, segment: Segment) -> Self {
        self.segments.push(segment);
        self
    }
}

impl RenderOnce for Segmented {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let palette = self.palette;
        // Snapshot of the segment list shared into the 'static Left/Right
        // handler. Each Segment's closures are already Rc, so cloning the Vec
        // is a chain of cheap Rc clones. The handler reads the LIVE `active`
        // flags off this snapshot, so consecutive arrow presses keep moving
        // (each press fires on_click → view updates → next render rebuilds the
        // snapshot with the new active segment).
        let nav: Rc<Vec<Segment>> = Rc::new(self.segments.clone());

        let mut track = div()
            .id(self.id)
            .flex()
            .flex_row()
            // The container owns the one border + radius + surface fill; the
            // segments sit flush inside it (no per-segment border/radius/gap),
            // so the track reads as one connected control instead of N loose
            // capsules. overflow_hidden clips the active/hovered segment fills
            // to the rounded outer corners (the focus ring is an outset shadow
            // on the track itself, NOT clipped by overflow).
            .rounded(palette.control_radius)
            .border_1()
            .border_color(palette.border)
            .bg(palette.surface)
            .overflow_hidden()
            // WCAG 2.4.7: one tab stop for the whole control. The ring mirrors
            // elements::focus_ring_shadow exactly (accent hue, 0.6 alpha, 2px
            // outset, 0 blur) and stays keyboard-gated: palette.ring already
            // carries the focus-visible decision in its alpha (1.0 on keyboard
            // renders, 0.0 otherwise), so 0.6 * palette.ring.a reproduces the
            // app-layer ring (0.6 keyboard / 0.0 pointer) without importing
            // elements (app↔ui cycle; button.rs follows the same palette.ring
            // idiom). 0.6 alpha matches every other focus ring in this chrome
            // row (tabs / sort cells / columns dropdown, all 0.6).
            .focusable()
            .tab_stop(true)
            .focus(|s| {
                s.shadow(vec![BoxShadow {
                    color: Rgba {
                        a: 0.6 * palette.ring.a,
                        ..palette.ring.into()
                    }
                    .into(),
                    offset: Point::default(),
                    blur_radius: px(0.0),
                    spread_radius: px(2.0),
                }])
            })
            // Left/Right = radiogroup nav (wrap, fire the target on_click).
            // stop_propagation keeps the gesture off the root key handler
            // (mirrors the sort-cell arrow handling in chrome.rs).
            .on_key_down(move |ev, win, cx| {
                let key = ev.keystroke.key.as_str();
                if key != "left" && key != "right" {
                    return;
                }
                let right = key == "right";
                let Some(cur) = nav.iter().position(|s| s.active) else {
                    return;
                };
                let n = nav.len();
                if n <= 1 {
                    return;
                }
                let next = if right {
                    (cur + 1) % n
                } else {
                    (cur + n - 1) % n
                };
                let click = nav[next].on_click.clone();
                click(win, cx);
                cx.stop_propagation();
            });

        for (i, seg) in self.segments.into_iter().enumerate() {
            let on_click = seg.on_click.clone();
            let on_hover = seg.on_hover.clone();
            let active = seg.active;
            let hovered = seg.hovered;
            let label = seg.label;
            let fill = fill_for(active, hovered);
            // Each segment is its own click/hover hit target (Stateful via
            // .id) but is NOT a tab stop / focusable — the container is the
            // single keyboard entry, so the mouse path and the keyboard
            // (arrow) path both land on the same on_click.
            let mut cell = div()
                .id(seg.id)
                .px(tokens::SPACE_10)
                .h(px(24.0))
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .cursor_pointer()
                .on_click(move |_ev, win, cx| on_click(win, cx))
                .on_hover(move |hov, win, cx| on_hover(hov, win, cx));
            // Hairline divider between adjacent segments (skip the first).
            if i > 0 {
                cell = cell.border_l_1().border_color(palette.border);
            }
            cell = match fill {
                SegmentFill::Active => cell
                    .bg(palette.accent)
                    .text_color(on_accent(palette.accent)),
                SegmentFill::Hovered => cell.bg(palette.hover).text_color(palette.fg),
                SegmentFill::Idle => cell.text_color(palette.fg),
            };
            track = track.child(cell.child(label));
        }
        track
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui_primitives_segmented_tests.rs"]
mod tests;
