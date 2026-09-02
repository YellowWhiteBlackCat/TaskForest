//! Shared visual, focus-ring, status-bar and text elements.

use gpui::{
    AnyElement, BoxShadow, Div, HighlightStyle, InteractiveElement, IntoElement, ParentElement,
    Point, StyleRefinement, Styled, StyledText, canvas, div, px,
};
use std::rc::Rc;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

use crate::gpui_app::graph::GraphCacheHandle;

// ── window-activation-aware chrome borders (Zed #2610) ────────────────────
// When the window is inactive, CSD chrome should recede behind the user's
// other windows: the 1px titlebar border drops to 60% alpha so the frame
// reads as system chrome instead of content. This mapping is the single
// source of truth for active→border strength; the render layer feeds it the
// live activation flag (`Window::is_window_active`, via the app's
// active-window handle) every frame.

/// Alpha multiplier applied to the titlebar border while the window is
/// inactive. 0.6 keeps the 1px line visible against the titlebar surface in
/// both modes while clearly weakening it (GNOME dims inactive decorations to
/// ~0.6 too).
const INACTIVE_BORDER_ALPHA: f32 = 0.6;

/// The titlebar border color under the current window activation state.
///
/// Active windows keep the theme's `border` token untouched — screenshot
/// baselines depend on the exact active look. Inactive windows render the
/// same hue at INACTIVE_BORDER_ALPHA alpha: the border tokens are opaque
/// hexes, so alpha scaling dims uniformly across all 8 skin variants without
/// inventing a per-theme dim color.
pub fn titlebar_border(t: &Theme, active: bool) -> gpui::Rgba {
    if active {
        taskmanager_ui::theme_binding::rgba(t.border)
    } else {
        taskmanager_ui::theme_binding::rgba(t.border.with_alpha(t.border.a * INACTIVE_BORDER_ALPHA))
    }
}

// ── focus ring (WCAG 2.4.7 Focus Visible) ──────────────────────────────────
// gpui 0.2.2 focus API, verified in gpui's src/elements/div.rs + src/window.rs:
//   * `.id(..)` on a `Div` → `Stateful<Div>`, which implements BOTH
//     `InteractiveElement` (the `.focus(|s| ..)` stateful style hook) and
//     `StatefulInteractiveElement` (`.focusable()` + `.tab_stop(true)`).
//   * `.focusable()` creates the persistent per-id `FocusHandle`, but does NOT by
//     itself enter the keyboard order; custom div controls also need
//     `.tab_stop(true)`. A stateless render fn needs no caller-side handle storage.
//   * `.focus(|s| ..)` is the focus analogue of `.hover(|s| ..)`: gpui applies
//     the refinement only when the element's tracked handle is focused
//     (div.rs:2490). `RootView` capture listeners track input modality once per
//     window; its Theme render snapshot lets this shared hook paint only for
//     keyboard focus without per-control state or thread-local storage.
//   * Bonus: a focusable element with `.on_click(..)` is ALSO keyboard-
//     activatable — gpui fires `ClickEvent::Keyboard` on Enter/Space (div.rs:2198),
//     so WCAG 2.1.1 (Keyboard) comes for free with 2.4.7.
//
// `FocusId` still carries no input origin. The application therefore derives
// focus-visible from root capture order: key capture selects Keyboard before Tab
// moves focus; pointer capture selects Pointer before a descendant click focuses.

/// The accent-colored focus ring drawn around a focused chrome control: a solid
/// 2px ring at ~0.6 alpha. Implemented as a 0-blur, 0-offset box-shadow with
/// +2px `spread_radius`, so the ring sits OUTSIDE the element and does NOT
/// perturb layout (unlike `border`, which would shove content inward by 2px the
/// frame focus lands). gpui 0.2.2's `BoxShadow` has no `inset` field, so this is
/// an outset ring — fine for chrome controls, which have titlebar breathing room
/// around them.
///
/// This is the raw [`BoxShadow`] value for callers composing a custom
/// [`StyleRefinement`]. For the common case prefer [`focus_ring`] — a drop-in
/// closure for gpui's `.focus(..)` hook. Both stay in theme via [`Theme::accent`]
/// so the ring adapts to all 8 skin variants.
pub fn focus_ring_shadow(t: &Theme) -> BoxShadow {
    BoxShadow {
        color: taskmanager_ui::theme_binding::hsla(t.accent.with_alpha(0.6)),
        offset: Point::default(),
        blur_radius: px(0.0),
        spread_radius: px(2.0),
    }
}

/// A drop-in adapter for gpui's `.focus(..)` stateful style hook: pass
/// `elements::focus_ring(t)` straight to `.focus(..)` on any `.focusable()`
/// element and a 2px accent ring appears around it while focused, with no
/// `FocusHandle` plumbing on the call site.
///
/// ```ignore
/// div()
///     .id("my-btn")
///     .focusable()
///     .tab_stop(true)
///     .focus(elements::focus_ring(t))
///     .on_click(..)
/// ```
///
/// See [`focus_ring_shadow`] for the ring geometry and the section header above
/// for the full gpui focus API + the focus-visible finding.
pub fn focus_ring(t: &Theme) -> impl FnOnce(StyleRefinement) -> StyleRefinement {
    let shadow = t.focus_visible().then(|| focus_ring_shadow(t));
    move |s: StyleRefinement| match shadow {
        Some(shadow) => s.shadow(vec![shadow]),
        None => s,
    }
}

/// Bottom status bar (Win11 TM / Mission Center parity): a border-top strip
/// with left-aligned summary parts and right-aligned readouts.
pub fn status_bar(theme: &Theme, left: &[String], right: &[String]) -> Div {
    div()
        .debug_selector(|| "tm-status-bar".into())
        .h(px(26.0))
        .flex_shrink_0()
        .border_t_1()
        .border_color(taskmanager_ui::theme_binding::hsla(theme.border))
        .px(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_12,
        ))
        .flex()
        .items_center()
        .gap(taskmanager_ui::theme_binding::definite_length(
            tokens::SPACE_16,
        ))
        .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
        .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
        .child(
            div()
                .flex()
                .items_center()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_16,
                ))
                .children(left.iter().map(|part| div().child(part.clone()))),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .items_center()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_16,
                ))
                .children(right.iter().map(|part| div().child(part.clone()))),
        )
}

/// Search-match highlighting: `text` with every `query` occurrence painted in
/// the theme's highlight token (`Theme::highlight_fg`, currently the accent;
/// kept as a distinct semantic so search legibility cannot drift from the
/// accent family) — ASCII case-insensitive, non-overlapping, via the SINGLE
/// shared match engine `taskmanager_core::core::text::match_ranges_ascii_ci`
/// (ADR-020: GPUI, TUI and iced all render these ranges and never recompute
/// matches themselves; the `taskmanager-ui` highlighter component remains for
/// other consumers but is no longer the GPUI search-match source).
/// Falls back to a plain text element when the query is empty.
pub fn highlighted_text(text: &str, query: &str, theme: &Theme) -> impl IntoElement {
    let query = query.trim();
    if query.is_empty() {
        return div().child(text.to_string()).into_any_element();
    }
    let matches = taskmanager_core::core::text::match_ranges_ascii_ci(text, query);
    highlighted_text_with_ranges(text, &matches, theme)
}

/// Range-driven variant of [`highlighted_text`] for callers that precompute
/// the match ranges once per projection change (the processes table's
/// visible-row projection keys them with the query; see
/// `processes_view::rows::VisibleRow::name_highlights`). A repaint then
/// renders the cached ranges instead of re-running the match engine per row
/// per frame.
pub fn highlighted_text_with_ranges(
    text: &str,
    ranges: &[std::ops::Range<usize>],
    theme: &Theme,
) -> AnyElement {
    // Keep the complete label in ONE text layout.  Building a flex `div` for
    // every match segment makes each segment an independent flex item: when a
    // table column becomes constrained, GPUI can wrap or shrink the segments
    // independently.  That was the source of names appearing one character
    // per line, or only the matching fragment remaining visible.
    //
    // `StyledText` applies the ranges after the parent text style has been
    // resolved, so the non-matching text keeps the cell's inherited color and
    // only the matching bytes receive the search color.  The matcher produces
    // non-overlapping UTF-8 byte ranges, which is exactly the contract expected
    // by `with_highlights`.
    let mut label = StyledText::new(text.to_string());
    if !ranges.is_empty() {
        let highlight = HighlightStyle {
            color: Some(taskmanager_ui::theme_binding::hsla(theme.highlight_fg())),
            ..Default::default()
        };
        label = label.with_highlights(ranges.iter().cloned().map(|range| (range, highlight)));
    }
    label.into_any_element()
}

/// A minimal sparkline: a thin polyline of `samples` (newest-last), NO axes, NO
/// fill, sized `w`×`h` px and stroked in `color`. Used for the per-row CPU trend in
/// the Processes table (see processes_view::proc_row).
///
/// **Empty `samples`** (cold-start — no history yet) render a flat baseline at the
/// vertical midpoint so the row height stays stable and nothing panics.
/// Non-finite samples are explicit provider gaps: finite runs retain their
/// original time positions and are never connected across the missing slots.
///
/// Vertical scale auto-ranges to the sample set's OWN max (≥ a tiny floor so an
/// all-zero history still draws the baseline) — standard sparkline behavior for a
/// tiny, axis-less mini-chart where two rows aren't meant to be compared in
/// amplitude. Drawn with the same `canvas` + `PathBuilder::stroke` primitives the
/// symbolic icons use (see magnifier_icon); NO fill, NO grid. Deliberately
/// hand-rolled rather than reusing graph_element
/// because that recipe always paints a filled area + grid and scales against a
/// fixed `max`, neither of which fits a 48×16 axis-less mini-chart.
///
/// `samples` accepts a `Vec<f32>`, `Rc<[f32]>`, or `&[f32]` (`Into<Rc<[f32]>>`);
/// the per-row projection passes the shared `Rc` so a repaint pays an `Rc` clone
/// instead of copying the history. Built stroke paths are cached across frames
/// by the sparkline scene store (see `graph::scene_cache`): frames that do not
/// move the canvas or change the data replay the cached paths, and dense runs
/// are LTTB-decimated to the pixel budget so the per-frame rebuilds that
/// horizontal scrolling forces stay cheap.
pub(crate) fn sparkline(
    samples: impl Into<Rc<[f32]>>,
    color: gpui::Rgba,
    w: f32,
    h: f32,
    cache: GraphCacheHandle,
) -> impl IntoElement {
    let samples: Rc<[f32]> = samples.into();
    canvas(
        |_bounds, _window, _cx| (),
        move |bounds, _t, window, _cx| {
            let mut cache = cache.borrow_mut();
            for path in cache.sparkline_paths(&samples, bounds, color) {
                window.paint_path(path, color);
            }
        },
    )
    .w(px(w))
    .h(px(h))
}
