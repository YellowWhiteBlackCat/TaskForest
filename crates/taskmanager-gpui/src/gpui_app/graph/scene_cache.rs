//! Persistent graph-scene replay cache for gpui 0.2.2's repaint model.
//!
//! gpui 0.2.2 has no damage region: every `window.refresh()` rebuilds the
//! whole element tree and re-runs every canvas paint closure, so a graph
//! whose samples and bounds did not change still re-ran
//! `build_graph_geometry` (Catmull-Rom controls + lyon tessellation of the
//! smooth fill and stroke) on EVERY frame — hover crosshair movement cost
//! one full re-tessellation per graph on the page. The caches that tried to
//! prevent this previously lived in `Rc<RefCell<..>>` slots created inside
//! the element constructors, which are themselves rebuilt every frame, so
//! they could never hit across frames.
//!
//! This module keeps the built geometry OUTSIDE the per-frame element tree,
//! in the window-owned [`GraphSceneCache`] keyed by everything that can change
//! the geometry:
//!
//! - the **identity** of the shared samples `Rc` (the generation-keyed view
//!   caches hand out a fresh `Rc` per telemetry tick, and the entry pins the
//!   `Rc` alive, so a pointer address can never be reused while it is a key);
//! - the canvas `bounds` (window space, so scroll/resize invalidate honestly);
//! - a field-wise fingerprint of the geometry-relevant `GraphOpts` fields and
//!   the base color — exact `PartialEq` on the packed fields, no hashing and
//!   therefore no collision risk.
//!
//! A cache hit replays the stored `Path`s through `window.paint_path`, which
//! still performs gpui's internal per-path vertex copy (see
//! shared architecture contract in `docs/ARCH.md`): replay is not zero-cost, but the
//! tessellation, spline math, and allocation churn of a rebuild are skipped.
//! The same module also carries the sparkline scene store (same contract),
//! the value-badge shaped-text cache, and the hover-refresh rate gate.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    App, Background, Bounds, Hsla, Path, PathBuilder, Pixels, Point, Rgba, Window, fill, point, px,
    size,
};

mod static_scene;

use static_scene::paint_graph_static_scene;

use super::{
    DualGraphSeries, GraphDynamicGeometry, GraphOpts, GraphStaticGeometry, GraphXMapping,
    build_graph_dynamic_geometry, build_graph_static_geometry, finite_sample_runs,
    graph_fill_background, latest_finite_sample,
};

/// Upper bound on retained graph scenes. Real pages stay far below this
/// (CPU page = N per-core minis + a handful of headline graphs); the cap only
/// exists so pathological windows (screenshot matrices, many-window runs)
/// cannot grow the cache without limit. Eviction first drops entries whose
/// samples `Rc` is held only by the store (a superseded telemetry
/// generation), then falls back to clearing everything.
pub(crate) const MAX_GRAPH_SCENE_ENTRIES: usize = 160;

/// Upper bound on retained graph grid/fill scenes. Static variants are much
/// fewer than dynamic data generations, but they still need a hard bound when
/// many window sizes or skins are exercised in one process.
pub(crate) const MAX_GRAPH_STATIC_SCENE_ENTRIES: usize = 64;

/// Upper bound on retained sparkline scenes (same eviction contract).
pub(crate) const MAX_SPARK_SCENE_ENTRIES: usize = 256;

/// Minimum wall time between two hover-driven `window.refresh()` calls.
/// gpui 0.2.2 repaints the whole window per refresh and mouse move events can
/// arrive far faster than any display (125 Hz–1 kHz devices), so an un-gated
/// crosshair schedules a full-window rebuild per event. 8 ms caps the rebuild
/// rate at 125 Hz, which every current display renders as smooth pointer
/// tracking while leaving the frame budget for the repaint itself.
pub(crate) const MIN_HOVER_REFRESH_INTERVAL: Duration = Duration::from_millis(8);

/// Exact bit-pattern fingerprint of an `Rgba` (f32 components have no
/// integers to compare; `to_bits` keeps NaN payloads honest too).
fn rgba_bits(color: Rgba) -> (u32, u32, u32, u32) {
    (
        color.r.to_bits(),
        color.g.to_bits(),
        color.b.to_bits(),
        color.a.to_bits(),
    )
}

/// Which series slot of a (possibly two-series) graph a dynamic scene stores.
/// Part of the dynamic key so a primary and a secondary series that share one
/// allocation identity (or one color) can never serve each other's entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SeriesSlot {
    /// The family-token series (single-series graphs always use this slot).
    Primary,
    /// The tinted second series of a two-series graph, painted UNDER the
    /// primary so the family-solid stroke sits on top at any crossing.
    Secondary,
}

/// Everything that can change a graph's painted geometry or baked-in colors.
///
/// Field-wise `PartialEq` (no hashing) makes a stale hit impossible: any
/// input change rebuilds. `samples_addr` is only meaningful because the
/// matching entry pins the `Rc` (see [`GraphSceneEntry::samples`]).
#[derive(Clone, Copy, PartialEq, Debug)]
struct GraphSceneKey {
    samples_addr: usize,
    samples_len: usize,
    data_revision: u64,
    origin: (f32, f32),
    size: (f32, f32),
    theme_key: (u32, u32, u32, u32),
    fill_alpha_bits: u32,
    grid_alpha_bits: u32,
    hlines: usize,
    vlines: usize,
    stroke_width_bits: u32,
    max_bits: u32,
    gradient_fill: bool,
    ref_lines: bool,
    smooth: bool,
    data_points: usize,
    x_mapping: GraphXMapping,
    series: SeriesSlot,
}

impl GraphSceneKey {
    /// Key constructor for an explicit series slot; the two-series paint path
    /// resolves the secondary slot through the same keying as the primary.
    fn for_series(
        samples: &Rc<[f32]>,
        bounds: Bounds<Pixels>,
        base: Rgba,
        opts: GraphOpts,
        series: SeriesSlot,
    ) -> Self {
        Self {
            samples_addr: Rc::as_ptr(samples).addr(),
            samples_len: samples.len(),
            data_revision: opts.animation_epoch,
            origin: (f32::from(bounds.origin.x), f32::from(bounds.origin.y)),
            size: (f32::from(bounds.size.width), f32::from(bounds.size.height)),
            theme_key: rgba_bits(base),
            fill_alpha_bits: opts.fill_alpha.to_bits(),
            grid_alpha_bits: opts.grid_alpha.to_bits(),
            hlines: opts.hlines,
            vlines: opts.vlines,
            stroke_width_bits: opts.stroke_width.to_bits(),
            max_bits: opts.max.to_bits(),
            gradient_fill: opts.gradient_fill,
            ref_lines: opts.ref_lines,
            smooth: opts.smooth,
            data_points: opts.data_points,
            x_mapping: if opts.sliding && samples.len() > opts.data_points {
                GraphXMapping::Slide
            } else {
                GraphXMapping::Normal
            },
            series,
        }
    }

    fn dynamic_key(self) -> GraphDynamicSceneKey {
        GraphDynamicSceneKey {
            samples_addr: self.samples_addr,
            samples_len: self.samples_len,
            data_revision: self.data_revision,
            origin: self.origin,
            size: self.size,
            theme_key: self.theme_key,
            fill_alpha_bits: self.fill_alpha_bits,
            stroke_width_bits: self.stroke_width_bits,
            max_bits: self.max_bits,
            gradient_fill: self.gradient_fill,
            smooth: self.smooth,
            data_points: self.data_points,
            x_mapping: self.x_mapping,
            series: self.series,
        }
    }
}

/// Inputs that can change the static grid, reference rules, or gradient/flat
/// fill style. Sample data and scale are deliberately absent: a new telemetry
/// revision should reuse this scene whenever the canvas and theme are stable.
#[derive(Clone, Copy, PartialEq, Debug)]
struct GraphStaticSceneKey {
    origin: (f32, f32),
    size: (f32, f32),
    theme_key: (u32, u32, u32, u32),
    fill_alpha_bits: u32,
    grid_alpha_bits: u32,
    hlines: usize,
    vlines: usize,
    stroke_width_bits: u32,
    gradient_fill: bool,
    ref_lines: bool,
}

/// Inputs that can change the dynamic filled-area and stroke paths. The
/// revision is kept in addition to the immutable sample allocation identity:
/// it makes the invalidation contract explicit and protects future callers
/// that may reuse a shared projection across accepted telemetry revisions.
/// `series` separates the primary and secondary slots of a two-series graph.
#[derive(Clone, Copy, PartialEq, Debug)]
struct GraphDynamicSceneKey {
    samples_addr: usize,
    samples_len: usize,
    data_revision: u64,
    origin: (f32, f32),
    size: (f32, f32),
    theme_key: (u32, u32, u32, u32),
    fill_alpha_bits: u32,
    stroke_width_bits: u32,
    max_bits: u32,
    gradient_fill: bool,
    smooth: bool,
    data_points: usize,
    x_mapping: GraphXMapping,
    series: SeriesSlot,
}

/// One cached graph scene: the pinned samples identity plus the built paths.
struct GraphSceneEntry {
    /// Pins the sample allocation the key's `samples_addr` points at: the
    /// address cannot be recycled by a newer `Rc` while this entry lives.
    samples: Rc<[f32]>,
    key: GraphDynamicSceneKey,
    geometry: GraphDynamicGeometry,
    /// Cached shaped text for the value badge (see [`ValueBadgeCache`]).
    badge: Option<ValueBadgeCache>,
}

/// One cached graph grid/fill scene. It has no sample ownership because it is
/// intentionally shared across data revisions.
struct GraphStaticSceneEntry {
    key: GraphStaticSceneKey,
    geometry: GraphStaticGeometry,
}

#[derive(Default)]
pub(super) struct GraphSceneCache {
    dynamic: Vec<GraphSceneEntry>,
    static_scenes: Vec<GraphStaticSceneEntry>,
    spark: Vec<SparkSceneEntry>,
}

/// Paint a graph through the replay cache: rebuild only when the samples
/// identity, bounds, colors, or geometry-relevant options changed since the
/// entry was stored; otherwise replay the stored paths. Static grid/fill
/// geometry is looked up separately so a data revision only rebuilds the
/// dynamic paths.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_graph_scene(
    cache: &mut GraphSceneCache,
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    samples: &Rc<[f32]>,
    base: Rgba,
    opts: GraphOpts,
    slide_offset: Pixels,
) {
    let fill = paint_graph_static_scene(cache, window, bounds, base, opts);
    paint_graph_dynamic_series(
        cache,
        window,
        cx,
        bounds,
        samples,
        base,
        opts,
        fill,
        SeriesSlot::Primary,
        slide_offset,
        Some(BadgeRequest::Value),
    );
}

/// Paint a two-series graph (a disk's read/write, a NIC's rx/tx) through the
/// same two-level replay cache. The grid/fill static scene is painted ONCE in
/// the primary's family color (drawing the faint grid twice would double its
/// alpha); the secondary's dynamic area is then resolved and painted UNDER
/// the primary's, each series keying its own dynamic entry through
/// [`SeriesSlot`] with its own samples identity, tint, and gap runs. The
/// value badge stays ONE pill per graph, in the family token — but its text
/// composes BOTH directions' newest values when both series carry labels
/// (see [`compose_badge_text`], mirroring iced's `readout_text`), so a
/// read/write or rx/tx graph never shows only one direction's current value.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_graph_dual_scene(
    cache: &mut GraphSceneCache,
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    primary: &DualGraphSeries<'_>,
    secondary: &DualGraphSeries<'_>,
    opts: GraphOpts,
    slide_offset: Pixels,
) {
    let fill = paint_graph_static_scene(cache, window, bounds, primary.base, opts);
    let secondary_fill = graph_fill_background(secondary.base, opts);
    paint_graph_dynamic_series(
        cache,
        window,
        cx,
        bounds,
        secondary.samples,
        secondary.base,
        opts,
        secondary_fill,
        SeriesSlot::Secondary,
        slide_offset,
        None,
    );
    let directions = match (primary.label, secondary.label) {
        (Some(primary_label), Some(secondary_label)) => Some(BadgeDirections {
            primary_label,
            secondary_samples: secondary.samples,
            secondary_label,
        }),
        // Without a label pair the pill falls back to the classic
        // single-value readout rather than guessing which curve is which.
        _ => None,
    };
    let badge = directions
        .as_ref()
        .map(BadgeRequest::Directions)
        .or(Some(BadgeRequest::Value));
    paint_graph_dynamic_series(
        cache,
        window,
        cx,
        bounds,
        primary.samples,
        primary.base,
        opts,
        fill,
        SeriesSlot::Primary,
        slide_offset,
        badge,
    );
}

/// Look up (or build and store) ONE series' dynamic scene and paint it,
/// translated by `slide_offset` when the slide animation is in progress.
/// `badge` limits the value badge to the series that owns the pill and
/// selects between the single-value and two-direction readouts.
#[allow(clippy::too_many_arguments)]
fn paint_graph_dynamic_series(
    cache: &mut GraphSceneCache,
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    samples: &Rc<[f32]>,
    base: Rgba,
    opts: GraphOpts,
    fill: Background,
    series: SeriesSlot,
    slide_offset: Pixels,
    badge: Option<BadgeRequest<'_>>,
) {
    let scene_key = GraphSceneKey::for_series(samples, bounds, base, opts, series);
    let key = scene_key.dynamic_key();
    let store = &mut cache.dynamic;
    let index = match store
        .iter()
        .position(|entry| entry.key == key && Rc::ptr_eq(&entry.samples, samples))
    {
        Some(index) => index,
        None => {
            evict_superseded(store, MAX_GRAPH_SCENE_ENTRIES);
            store.push(GraphSceneEntry {
                samples: Rc::clone(samples),
                key,
                geometry: build_graph_dynamic_geometry(
                    bounds,
                    samples.as_ref(),
                    base,
                    opts,
                    fill,
                    scene_key.x_mapping,
                ),
                badge: None,
            });
            store.len() - 1
        }
    };
    let entry = &mut store[index];
    if slide_offset == px(0.0) {
        entry.geometry.paint(window);
    } else {
        entry
            .geometry
            .paint_translated(window, Point::new(slide_offset, px(0.0)));
    }
    if let Some(request) = badge.filter(|_| opts.value_badge) {
        let directions = match request {
            BadgeRequest::Value => None,
            BadgeRequest::Directions(directions) => Some(directions),
        };
        let font_size = window.text_style().font_size;
        draw_value_badge(
            window,
            cx,
            BadgePaintInputs {
                bounds,
                samples: samples.as_ref(),
                base,
                opts,
                font_size,
                directions,
            },
            &mut entry.badge,
        );
    }
}

/// Static scenes do not carry a sample handle that can be used for
/// supersession, so use a simple bounded store. The steady-state path never
/// reaches this helper; it only runs when a new static variant is inserted.
fn evict_static(store: &mut Vec<GraphStaticSceneEntry>, cap: usize) {
    if store.len() >= cap {
        store.clear();
    }
}

/// Drop entries whose samples series nothing outside the store references
/// (a superseded telemetry generation), then clear the store entirely if it
/// is still at capacity. Runs only on the miss path, so a steady-state page
/// never pays it.
fn evict_superseded<Entry>(store: &mut Vec<Entry>, cap: usize)
where
    Entry: StoresSamples,
{
    store.retain(|entry| Rc::strong_count(entry.samples()) > 1);
    if store.len() >= cap {
        store.clear();
    }
}

/// Entry view needed by [`evict_superseded`]: the pinned samples series.
trait StoresSamples {
    fn samples(&self) -> &Rc<[f32]>;
}

impl StoresSamples for GraphSceneEntry {
    fn samples(&self) -> &Rc<[f32]> {
        &self.samples
    }
}

/// Cached text shaping for a graph's newest-value badge. Cursor movement and
/// hover frames do not change the newest sample, so formatting and glyph
/// layout stay out of the repaint path until the data, theme color, ambient
/// font size, or — on a two-direction pill — the secondary direction's
/// newest sample or a label change does. (The ambient font family is not
/// fingerprinted by gpui 0.2.2's `TextStyle`; a family change without a size
/// or color change falls back to one stale-shaped badge until the next data
/// tick.)
struct ValueBadgeCache {
    latest_bits: Option<u32>,
    secondary_latest_bits: Option<u32>,
    label_key: Option<((usize, usize), (usize, usize))>,
    theme_key: (u32, u32, u32, u32),
    badge_fmt: Option<fn(f32) -> String>,
    font_size: gpui::AbsoluteLength,
    line: gpui::ShapedLine,
}

fn same_badge_formatter(left: Option<fn(f32) -> String>, right: Option<fn(f32) -> String>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => std::ptr::fn_addr_eq(left, right),
        _ => false,
    }
}

/// Stable identity of one label string: labels come from the i18n table, so
/// the (address, length) pair distinguishes a locale switch or a wording
/// change without comparing the bytes on every paint.
fn badge_label_key(label: &str) -> (usize, usize) {
    (label.as_ptr().addr(), label.len())
}

/// The two-direction inputs of a dual-series graph's value badge: each
/// direction's label plus the secondary series' samples (the primary's
/// samples already flow through the series paint that owns the pill).
struct BadgeDirections<'a> {
    primary_label: &'a str,
    secondary_samples: &'a [f32],
    secondary_label: &'a str,
}

/// Which readout one series paint's value badge shows: the classic
/// single-value pill, or a dual graph's two-direction composition.
enum BadgeRequest<'a> {
    Value,
    Directions(&'a BadgeDirections<'a>),
}

/// Compose the value badge's text. The single-series pill reads out the
/// newest value alone; the two-direction pill prefixes each direction's
/// label ("Read 1.2 MB/s · Write 340 kB/s") so the reader can tell which
/// curve is which — the same composition iced's `readout_text` uses for its
/// hover pill. A direction whose newest sample is a gap stays silent instead
/// of fabricating a value; when neither direction has evidence there is
/// nothing to read out. Pure so the composition is unit-tested headlessly.
fn compose_badge_text(
    primary: Option<f32>,
    directions: Option<&BadgeDirections<'_>>,
    fmt: Option<fn(f32) -> String>,
) -> Option<String> {
    let value_text = |value: f32| match fmt {
        Some(fmt) => fmt(value),
        None => format!("{value:.1}"),
    };
    let Some(directions) = directions else {
        return primary.map(value_text);
    };
    let secondary = latest_finite_sample(directions.secondary_samples);
    let mut parts = Vec::with_capacity(2);
    if let Some(primary) = primary {
        parts.push(format!(
            "{} {}",
            directions.primary_label,
            value_text(primary)
        ));
    }
    if let Some(secondary) = secondary {
        parts.push(format!(
            "{} {}",
            directions.secondary_label,
            value_text(secondary)
        ));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

/// Paint inputs for the value badge, bundled to keep
/// [`draw_value_badge`]'s argument list under the lint budget.
struct BadgePaintInputs<'a> {
    bounds: Bounds<Pixels>,
    samples: &'a [f32],
    base: Rgba,
    opts: GraphOpts,
    font_size: gpui::AbsoluteLength,
    directions: Option<&'a BadgeDirections<'a>>,
}

/// Paint the [`GraphOpts::value_badge`] pill (moved here from the per-frame
/// element caches). Shaping is cached on the entry; only the quad and the
/// shaped-line replay happen per frame. The pill anchors to the top-right
/// corner and clamps inside the canvas, so a wide two-direction readout on a
/// narrow graph overlays data instead of drawing off-canvas.
fn draw_value_badge(
    window: &mut Window,
    cx: &mut App,
    inputs: BadgePaintInputs<'_>,
    cache: &mut Option<ValueBadgeCache>,
) {
    let BadgePaintInputs {
        bounds,
        samples,
        base,
        opts,
        font_size,
        directions,
    } = inputs;
    let latest = latest_finite_sample(samples);
    let secondary_latest = directions.and_then(|dirs| latest_finite_sample(dirs.secondary_samples));
    if latest.is_none() && secondary_latest.is_none() {
        return;
    }
    let latest_bits = latest.map(f32::to_bits);
    let secondary_latest_bits = secondary_latest.map(f32::to_bits);
    let label_key = directions.map(|dirs| {
        (
            badge_label_key(dirs.primary_label),
            badge_label_key(dirs.secondary_label),
        )
    });
    let theme_key = rgba_bits(base);
    let needs_shape = cache.as_ref().is_none_or(|cached| {
        cached.latest_bits != latest_bits
            || cached.secondary_latest_bits != secondary_latest_bits
            || cached.label_key != label_key
            || cached.theme_key != theme_key
            || !same_badge_formatter(cached.badge_fmt, opts.badge_fmt)
            || cached.font_size != font_size
    });
    if needs_shape {
        let Some(text) = compose_badge_text(latest, directions, opts.badge_fmt) else {
            return;
        };
        let mut run = window.text_style().to_run(text.len());
        run.color = Hsla::from(base);
        run.font.weight = taskmanager_ui::theme_binding::font_weight(
            taskmanager_theme::tokens::FONT_WEIGHT_SEMIBOLD,
        );
        let line = window
            .text_system()
            .shape_line(text.into(), px(10.0), &[run], None);
        *cache = Some(ValueBadgeCache {
            latest_bits,
            secondary_latest_bits,
            label_key,
            theme_key,
            badge_fmt: opts.badge_fmt,
            font_size,
            line,
        });
    }
    let Some(cached) = cache.as_ref() else {
        return;
    };
    let text_w = cached.line.width;
    let line_height = px(12.0);
    let pad_x = px(4.0);
    let pad_y = px(1.5);

    let badge_w = text_w + pad_x * 2.0;
    let badge_h = line_height + pad_y * 2.0;
    let badge_bounds = Bounds {
        origin: point(
            (bounds.origin.x + bounds.size.width - badge_w - px(2.0))
                .max(bounds.origin.x + px(2.0)),
            bounds.origin.y + px(2.0),
        ),
        size: size(badge_w, badge_h),
    };
    window.paint_quad(fill(badge_bounds, Hsla::from(base).opacity(0.18)));
    let _ = cached.line.paint(
        point(badge_bounds.origin.x + pad_x, badge_bounds.origin.y + pad_y),
        line_height,
        window,
        cx,
    );
}

// ── Sparkline scene store ──────────────────────────────────────────────────

/// Everything that can change a sparkline's built stroke paths.
#[derive(Clone, Copy, PartialEq, Debug)]
struct SparkSceneKey {
    samples_addr: usize,
    samples_len: usize,
    origin: (f32, f32),
    size: (f32, f32),
    color_bits: (u32, u32, u32, u32),
}

/// One cached sparkline scene (see [`GraphSceneEntry`] for the pinning
/// contract). Sparklines render inside horizontally scrolled tables, so
/// their window-space origin changes with the scroll offset and they
/// intentionally rebuild per scrolled frame — the store exists for
/// hover/vertical-scroll frames and telemetry-stable repaints, and the
/// per-pixel decimation below keeps the scrolled rebuilds cheap.
struct SparkSceneEntry {
    samples: Rc<[f32]>,
    key: SparkSceneKey,
    paths: Vec<Path<Pixels>>,
}

impl StoresSamples for SparkSceneEntry {
    fn samples(&self) -> &Rc<[f32]> {
        &self.samples
    }
}

/// Number of retained samples per horizontal pixel above which a run is
/// LTTB-decimated: a 1 px stroke cannot resolve denser detail, and the extra
/// points only feed the per-frame tessellation cost during horizontal
/// scrolling.
pub(crate) const SPARKLINE_SAMPLES_PER_PIXEL: usize = 2;

/// Build (or replay) a sparkline's stroke paths for `samples` at `bounds`,
/// stroked in `color`. Pure geometry; the caller owns painting.
pub(super) fn sparkline_paths(
    cache: &mut GraphSceneCache,
    samples: &Rc<[f32]>,
    bounds: Bounds<Pixels>,
    color: Rgba,
) -> Vec<Path<Pixels>> {
    let key = SparkSceneKey {
        samples_addr: Rc::as_ptr(samples).addr(),
        samples_len: samples.len(),
        origin: (f32::from(bounds.origin.x), f32::from(bounds.origin.y)),
        size: (f32::from(bounds.size.width), f32::from(bounds.size.height)),
        color_bits: rgba_bits(color),
    };
    let store = &mut cache.spark;
    if let Some(entry) = store
        .iter()
        .find(|entry| entry.key == key && Rc::ptr_eq(&entry.samples, samples))
    {
        return entry.paths.clone();
    }
    evict_superseded(store, MAX_SPARK_SCENE_ENTRIES);
    let paths = build_sparkline_paths(samples, bounds, SPARKLINE_SAMPLES_PER_PIXEL);
    store.push(SparkSceneEntry {
        samples: Rc::clone(samples),
        key,
        paths: paths.clone(),
    });
    paths
}

/// Build the sparkline stroke paths: a flat midpoint baseline for an empty
/// history, otherwise one thin polyline per finite run with original time
/// indices (gaps never connect). Denser runs than
/// `samples_per_pixel` × canvas width are first decimated with LTTB, which
/// keeps the first and last sample and preserves the original indices so the
/// x mapping and gap semantics are unchanged.
fn build_sparkline_paths(
    samples: &[f32],
    bounds: Bounds<Pixels>,
    samples_per_pixel: usize,
) -> Vec<Path<Pixels>> {
    let o = bounds.origin;
    let bw = f32::from(bounds.size.width).max(1.0);
    let bh = f32::from(bounds.size.height).max(1.0);
    let sw = 1.0_f32; // thin stroke
    let mid = bh * 0.5;
    // Half-band minus half-stroke so the polyline never clips at the edges.
    let amp = (bh * 0.5 - sw * 0.5).max(0.0);
    // Auto-range to finite observations only (floor so measured all-zero
    // data remains a valid baseline).
    let max = samples
        .iter()
        .copied()
        .filter(|sample| sample.is_finite())
        .fold(0.0f32, f32::max)
        .max(1e-6);
    let y_for = |sample: f32| mid - (sample / max) * amp;
    if samples.is_empty() {
        let mut path = PathBuilder::stroke(px(sw));
        path.move_to(point(o.x + px(0.0), o.y + px(mid)));
        path.line_to(point(o.x + px(bw), o.y + px(mid)));
        return path.build().ok().into_iter().collect();
    }
    let denom = (samples.len() - 1).max(1) as f32;
    let point_budget = (bw.ceil() as usize)
        .saturating_mul(samples_per_pixel)
        .max(2);
    let mut paths = Vec::new();
    for run in finite_sample_runs(samples) {
        let run = decimate_run(&run, point_budget);
        let mut path = PathBuilder::stroke(px(sw));
        if let [(index, sample)] = run.as_slice() {
            let x = (*index as f32 / denom) * bw;
            let y = y_for(*sample);
            path.move_to(point(o.x + px((x - 0.5).max(0.0)), o.y + px(y)));
            path.line_to(point(o.x + px((x + 0.5).min(bw)), o.y + px(y)));
        } else {
            for (offset, (index, sample)) in run.into_iter().enumerate() {
                let x = (index as f32 / denom) * bw;
                let point = point(o.x + px(x), o.y + px(y_for(sample)));
                if offset == 0 {
                    path.move_to(point);
                } else {
                    path.line_to(point);
                }
            }
        }
        if let Ok(path) = path.build() {
            paths.push(path);
        }
    }
    paths
}

/// Largest-triangle-three-buckets downsampling of one finite run, delegated
/// to the toolkit-neutral single source
/// (`taskmanager_application::history_decimation::lttb_indices`, promoted
/// verbatim from this crate). Always keeps the first and last sample;
/// interior buckets keep the point with the largest triangle area against the
/// previous selection and the next bucket's average. Original indices are
/// preserved (the time axis and gap spacing never distort). Runs at or below
/// `budget` — and degenerate budgets (`< 3`) — are returned unchanged. The
/// thin adapter exists because the sparkline path consumes `(index, sample)`
/// pairs while the neutral kernel returns run positions.
fn decimate_run(run: &[(usize, f32)], budget: usize) -> Vec<(usize, f32)> {
    taskmanager_application::history_decimation::lttb_indices(run, budget)
        .into_iter()
        .map(|position| run[position])
        .collect()
}

// ── Hover-refresh rate gate ────────────────────────────────────────────────

/// Pure decision core of the hover-refresh gate, factored out so the interval
/// rule is testable without sleeping: a refresh is due when none happened yet
/// or at least `min_interval` elapsed since the last one.
pub(super) fn hover_refresh_is_due(
    last: Option<Instant>,
    now: Instant,
    min_interval: Duration,
) -> bool {
    last.is_none_or(|last| {
        now.checked_duration_since(last)
            .is_some_and(|elapsed| elapsed >= min_interval)
    })
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_graph_scene_cache_tests.rs"]
mod tests;
