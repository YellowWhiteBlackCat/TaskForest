//! Mission Center `GraphWidget` recipe, rendered via gpui's canvas + PathBuilder.
//!
//! Each graph is a 60-point scrolling window drawn as a FILLED AREA (FillToBottom) in the
//! category base color at ~39% opacity, with a solid 1px stroke of the same hue on top, over
//! a faint grid. This is the single most recognizable Mission Center visual.

use gpui::{
    AnyElement, Background, Bounds, ElementId, Hsla, IntoElement, ParentElement, Path, PathBuilder,
    Pixels, Point, Rgba, Styled, Window, canvas, div, linear_color_stop, linear_gradient, point,
    px,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

mod hover;
pub(crate) mod scene_cache;
pub(crate) mod slide;
pub use hover::{GraphHover, GraphSecondarySeries, graph_hover};
pub(crate) use hover::{graph_element_hover, graph_element_hover_dual};

use slide::slide_progress;

pub(crate) const MIN_GRAPH_DATA_POINTS: usize = 10;
pub(crate) const MAX_GRAPH_DATA_POINTS: usize = 600;
pub(crate) const DEFAULT_GRAPH_DATA_POINTS: usize = 60;
pub(crate) const DEFAULT_GRAPH_DATA_POINTS_CONFIG: u32 = 60;

/// Per-window Performance graph preferences projected from `core::Config`.
/// Providers publish the full bounded history; this type only controls how
/// the GPUI presentation slices, scales, smooths, and animates it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GraphSettings {
    pub(crate) data_points: usize,
    pub(crate) sliding_graphs: bool,
    pub(crate) network_dynamic_scaling: bool,
    pub(crate) animation_epoch: u64,
}

impl Default for GraphSettings {
    fn default() -> Self {
        Self {
            data_points: DEFAULT_GRAPH_DATA_POINTS,
            sliding_graphs: false,
            network_dynamic_scaling: true,
            animation_epoch: 0,
        }
    }
}

impl GraphSettings {
    pub(crate) fn from_config(
        data_points: u32,
        sliding_graphs: bool,
        network_dynamic_scaling: bool,
        animation_epoch: u64,
    ) -> Self {
        let data_points = usize::try_from(data_points).unwrap_or(MAX_GRAPH_DATA_POINTS);
        Self {
            data_points: Self::clamp_data_points(data_points),
            sliding_graphs,
            network_dynamic_scaling,
            animation_epoch,
        }
    }

    pub(crate) const fn clamp_data_points(value: usize) -> usize {
        if value < MIN_GRAPH_DATA_POINTS {
            MIN_GRAPH_DATA_POINTS
        } else if value > MAX_GRAPH_DATA_POINTS {
            MAX_GRAPH_DATA_POINTS
        } else {
            value
        }
    }

    pub(crate) fn data_points_as_config(self) -> u32 {
        u32::try_from(self.data_points).unwrap_or(u32::MAX)
    }
}

/// Tunables for one graph, matching the MC recipe.
///
/// The baseline fields below reproduce the original Mission Center filled-area
/// look exactly. The opt-in fields (`gradient_fill`, `ref_lines`, `value_badge`,
/// `badge_fmt`, `smooth`) are all **off by default** — existing callers render
/// pixel-identical to before unless they explicitly enable one. They are the
/// follow-up levers for a more refined Win11-TM / MC-grade aesthetic.
#[derive(Clone, Copy)]
pub struct GraphOpts {
    /// Fill area opacity (MC ~= 100/255 ~= 0.39).
    pub fill_alpha: f32,
    /// Grid line opacity (faint).
    pub grid_alpha: f32,
    /// Number of horizontal grid divisions.
    pub hlines: usize,
    /// Number of vertical grid divisions.
    pub vlines: usize,
    /// Stroke width in pixels.
    pub stroke_width: f32,
    /// Value scale (e.g. 100.0 for utilization %, or a max throughput/temp).
    pub max: f32,
    // ── opt-in refinements (all default OFF; no effect unless toggled) ──────
    /// When true, replace the flat `fill_alpha` wash with a vertical gradient
    /// (line color @ ~0.35 alpha at the top of the area → transparent at the
    /// graph floor). Uses gpui's `linear_gradient`, which is mapped to the fill
    /// path's bounding box by the renderer.
    pub gradient_fill: bool,
    /// When true, additionally stroke 4 emphasized horizontal reference rules
    /// at 25/50/75/100% of `max` in a low-contrast tint of `base` (MC shows these
    /// quarter/half/three-quarter emphasis lines over the base grid).
    pub ref_lines: bool,
    /// When true, paint a small unobtrusive badge at the top-right of the graph
    /// showing the newest sample value (formatted via `badge_fmt`, or a plain
    /// `{:.1}` when `badge_fmt` is `None`). Rendered via the gpui text system, so
    /// it needs `&mut App` — painted in the canvas closure, not in `draw_graph`.
    pub value_badge: bool,
    /// Optional formatter for the [`Self::value_badge`] text. `fn(f32) -> String`
    /// is `Copy`, so the `Option` stays `Copy` and `#[derive(Clone, Copy)]` on
    /// `GraphOpts` is preserved. e.g. `Some(|v| format!("{:.0}%", v))`.
    pub badge_fmt: Option<fn(f32) -> String>,
    /// When true, draw the polyline (and the top edge of its fill) through a
    /// light Catmull-Rom spline instead of straight segments, for a smoother
    /// curve. Tension is fixed at SMOOTH_TENSION (subtle).
    /// Catmull-Rom curve smoothing — ON by default: the elegant curve is the
    /// product's rendering quality bar (Win11 TM / Mission Center smooth
    /// unconditionally; there is no "angular lines" mode to expose).
    pub smooth: bool,
    /// Number of newest samples to project into this graph.
    pub data_points: usize,
    /// When true, animate the graph refresh transition.
    pub sliding: bool,
    /// Monotonic data revision used to restart the refresh animation.
    pub animation_epoch: u64,
}

impl Default for GraphOpts {
    fn default() -> Self {
        Self {
            fill_alpha: 0.39,
            grid_alpha: 0.13,
            hlines: 5,
            vlines: 6,
            stroke_width: 1.5,
            max: 100.0,
            // All refinements OFF → existing callers render exactly as before.
            gradient_fill: false,
            ref_lines: false,
            value_badge: false,
            badge_fmt: None,
            smooth: true,
            // A bare GraphOpts is also used by System dashboard history cards,
            // whose timeline already owns its window length. Performance pages
            // apply the Mission Center preference explicitly via with_settings.
            data_points: MAX_GRAPH_DATA_POINTS,
            sliding: false,
            animation_epoch: 0,
        }
    }
}

impl GraphOpts {
    /// Apply the shared Performance preference projection at the render edge.
    /// Callers can still provide per-graph scale, color, badge, and reference
    /// line choices without duplicating the user preference mapping.
    pub(crate) fn with_settings(mut self, settings: GraphSettings) -> Self {
        // `smooth` stays at its default TRUE — the elegant curve is intrinsic
        // rendering, not a preference (see the field doc).
        self.data_points = settings.data_points;
        self.sliding = settings.sliding_graphs;
        self.animation_epoch = settings.animation_epoch;
        self
    }
}

/// The graph-element variant of the slice-tail limit helper: callers often pre-limit
/// (the Performance layout does, for its summary row), so when the window
/// already fits the samples are reused as-is — the old unconditional
/// `limit_samples` here copied the whole window a second time per element.
/// Tail-limit a SHARED series without copying when it already fits. The
/// generation-keyed caches hand out rings already capped at
/// `MAX_GRAPH_DATA_POINTS` (and the data-points setting clamps to the same
/// bound), so the common case is the identity: the caller keeps the same `Rc`
/// and no per-frame copy happens. Only a series longer than the configured
/// window pays the tail-slice.
pub fn latest_samples_rc(samples: Rc<[f32]>, data_points: usize) -> Rc<[f32]> {
    let limit = GraphSettings::clamp_data_points(data_points);
    if samples.len() <= limit {
        samples
    } else {
        Rc::from(&samples[samples.len() - limit..])
    }
}

/// One memoized tail slice: the source projection plus the exact `limit`
/// it was cut to. The source `Rc` is pinned so a recycled address can never
/// serve a stale slice.
struct SlideSliceEntry {
    source: Rc<[f32]>,
    limit: usize,
    slice: Rc<[f32]>,
}

/// All mutable graph presentation caches owned by one GPUI window.
///
/// The handle lives on `RootView` and is cloned into canvas closures. Keeping
/// the caches here makes their lifetime and isolation explicit: a second
/// window cannot observe a first window's graph scenes, slide clocks, sample
/// projections, or hover-refresh budget, while the closures still outlive the
/// render call safely.
#[derive(Default)]
pub(crate) struct GraphPresentationCache {
    tail_slices: Vec<SlideSliceEntry>,
    scenes: scene_cache::GraphSceneCache,
    slides: slide::SlideCache,
    samples: crate::gpui_app::history_samples::DeviceSampleCache,
    last_hover_refresh: Option<Instant>,
}

pub(crate) type GraphCacheHandle = Rc<RefCell<GraphPresentationCache>>;

#[must_use]
pub(crate) fn new_graph_cache() -> GraphCacheHandle {
    Rc::new(RefCell::new(GraphPresentationCache::default()))
}

impl GraphPresentationCache {
    pub(super) fn latest_samples(
        &mut self,
        samples: Rc<[f32]>,
        data_points: usize,
        sliding: bool,
    ) -> Rc<[f32]> {
        let limit = GraphSettings::clamp_data_points(data_points).saturating_add(if sliding {
            1
        } else {
            0
        });
        if samples.len() <= limit {
            return samples;
        }
        if let Some(entry) = self
            .tail_slices
            .iter()
            .find(|entry| entry.limit == limit && Rc::ptr_eq(&entry.source, &samples))
        {
            return Rc::clone(&entry.slice);
        }
        if self.tail_slices.len() >= 512 {
            self.tail_slices
                .retain(|entry| Rc::strong_count(&entry.source) > 1);
            if self.tail_slices.len() >= 512 {
                self.tail_slices.clear();
            }
        }
        let slice = Rc::from(&samples[samples.len() - limit..]);
        self.tail_slices.push(SlideSliceEntry {
            source: samples,
            limit,
            slice: Rc::clone(&slice),
        });
        slice
    }

    fn scenes_mut(&mut self) -> &mut scene_cache::GraphSceneCache {
        &mut self.scenes
    }

    fn slides_mut(&mut self) -> &mut slide::SlideCache {
        &mut self.slides
    }

    pub(crate) fn sparkline_paths(
        &mut self,
        samples: &Rc<[f32]>,
        bounds: Bounds<Pixels>,
        color: Rgba,
    ) -> Vec<Path<Pixels>> {
        scene_cache::sparkline_paths(self.scenes_mut(), samples, bounds, color)
    }

    pub(crate) fn with_device_samples<R>(
        &mut self,
        access: impl FnOnce(&mut crate::gpui_app::history_samples::DeviceSampleCache) -> R,
    ) -> R {
        access(&mut self.samples)
    }

    pub(super) fn hover_refresh_due(&mut self, now: Instant) -> bool {
        let due = scene_cache::hover_refresh_is_due(
            self.last_hover_refresh,
            now,
            scene_cache::MIN_HOVER_REFRESH_INTERVAL,
        );
        if due {
            self.last_hover_refresh = Some(now);
        }
        due
    }

    pub(super) fn reset_hover_refresh(&mut self) {
        self.last_hover_refresh = None;
    }
}

/// Whether a graph has the extra older sample required for a natural slide.
/// Without it, translating the curve would only reveal blank space on the
/// right — the “shrinking back” artifact the first implementation showed.
fn graph_slide_supported(samples: &[f32], data_points: usize) -> bool {
    samples.len() > GraphSettings::clamp_data_points(data_points)
}

/// Horizontal distance one sample slot occupies in a `data_points`-wide
/// graph. The slide moves exactly one of these slots; the settled frame
/// matches the static map (`width / (capacity - 1)`).
fn graph_slide_spacing(bounds: Bounds<Pixels>, data_points: usize) -> Pixels {
    let capacity = GraphSettings::clamp_data_points(data_points).max(1);
    let denom = capacity.saturating_sub(1).max(1) as f32;
    px(f32::from(bounds.size.width) / denom)
}

/// X coordinate for the sliding base geometry.
///
/// The base puts the extra older sample at the left edge and the current
/// newest sample one slot beyond the right edge (`capacity` samples occupy
/// the full width, so the `capacity + 1`-th is off-screen). Painting then
/// translates the cached paths left by `progress * slot`: progress 0 shows
/// the previous window, progress 1 shows the current window.
fn sample_x_slide(
    left: Pixels,
    width: Pixels,
    index: usize,
    data_points: usize,
    progress: f32,
) -> Pixels {
    let capacity = GraphSettings::clamp_data_points(data_points).max(1);
    let denom = capacity.saturating_sub(1).max(1) as f32;
    let slot = f32::from(width) / denom;
    px(f32::from(left) + (index as f32 - progress.clamp(0.0, 1.0)) * slot)
}

/// Catmull-Rom tension used by [`GraphOpts::smooth`]. `0.0` = straight segments
/// (degenerate), `1.0` = standard uniform Catmull-Rom; `0.5` is a gentle smoothing
/// that still tracks sharp transitions without overshoot ringing.
const SMOOTH_TENSION: f32 = 0.5;

/// How far a two-series graph lifts the secondary series' color toward white
/// from the device-family token — iced-parity (`device_chart::multi`'s
/// `SECONDARY_TINT_LIFT`): a tint of the SAME color (solid "read"/"receive"
/// vs light "write"/"send"), never a new product color.
pub(crate) const SECONDARY_TINT_LIFT: f32 = 0.32;

/// The (primary, secondary) color pair for one device family's two-series
/// graph: the family token as-is, and the same token lifted toward white by
/// [`SECONDARY_TINT_LIFT`]. Pure so the "no new product color" rule is
/// unit-tested headlessly (the secondary is strictly between the base and
/// white on every channel, alpha untouched).
#[must_use]
pub(crate) fn dual_series_colors(base: Rgba) -> (Rgba, Rgba) {
    let lift = |channel: f32| channel + (1.0 - channel) * SECONDARY_TINT_LIFT;
    (
        base,
        Rgba {
            r: lift(base.r),
            g: lift(base.g),
            b: lift(base.b),
            a: base.a,
        },
    )
}

/// One borrowed series of a two-series graph paint: the (already
/// window-limited) samples, the series color, and the direction's label
/// ("Read"/"Write", "Receive"/"Send"). The owning element holds the `Rc`s and
/// the label strings alive; this view only feeds the scene cache.
pub(crate) struct DualGraphSeries<'a> {
    pub(crate) samples: &'a Rc<[f32]>,
    pub(crate) base: Rgba,
    pub(crate) label: Option<&'a str>,
}

/// The fill background a series' area uses: the opt-in vertical gradient, or
/// the flat `fill_alpha` wash. Shared by the static grid scene (primary
/// series) and the two-series paint path so the secondary's fill derives from
/// its own tint through the exact same rule.
pub(crate) fn graph_fill_background(base: Rgba, opts: GraphOpts) -> Background {
    if opts.gradient_fill {
        linear_gradient(
            180.0,
            linear_color_stop(Hsla::from(base).opacity(0.35), 0.0),
            linear_color_stop(Hsla::from(base).opacity(0.0), 1.0),
        )
    } else {
        Background::from(Rgba {
            a: opts.fill_alpha,
            ..base
        })
    }
}

/// Mission Center `compute_column_count`: choose grid columns for N logical processors.
/// if N<=3 -> N; else the first divisor of N in [round(sqrt(N)), min(N, 2*round(sqrt(N)))].
/// e.g. N=20 -> 4 cols (4x5), N=24 -> 6 cols (6x4).
pub fn compute_column_count(n: usize) -> usize {
    if n <= 3 {
        return n.max(1);
    }
    let s = ((n as f64).sqrt().round() as usize).max(1);
    let upper = n.min(s * 2);
    (s..=upper).find(|&c| n.is_multiple_of(c)).unwrap_or(s)
}

/// Paint a filled-area graph into `bounds` using the given scrolling
/// `samples` (oldest..newest).
///
/// Modern task-manager x mapping: the newest sample pins to the right edge and
/// every sample owns a fixed slot of the window (`width / capacity`), so a
/// freshly opened graph grows a short, evenly-paced curve from the right edge
/// leftward — never a two-sample line stretched across the full width. Once the
/// window fills (`n >= capacity`) the mapping becomes the usual full-span
/// spread. Drawing and hover use this one mapping, so a tooltip can never
/// disagree with the visible sample.
#[must_use]
pub(crate) fn sample_x(
    left: Pixels,
    width: Pixels,
    index: usize,
    n: usize,
    capacity: usize,
) -> Pixels {
    let denom = n.saturating_sub(1).max(capacity.saturating_sub(1)).max(1) as f32;
    let slots_from_right = (n - 1 - index) as f32;
    px(f32::from(left) + f32::from(width) * (1.0 - slots_from_right / denom))
}

#[derive(Clone)]
pub(crate) struct GraphGeometry {
    static_geometry: GraphStaticGeometry,
    dynamic_geometry: GraphDynamicGeometry,
}

#[derive(Clone)]
pub(crate) struct GraphStaticGeometry {
    paints: Vec<GraphPaint>,
    fill: Background,
}

#[derive(Clone)]
pub(crate) struct GraphDynamicGeometry {
    paints: Vec<GraphPaint>,
}

#[derive(Clone)]
struct GraphPaint {
    path: Path<Pixels>,
    color: Background,
}

impl GraphStaticGeometry {
    pub(crate) fn paint(&self, window: &mut Window) {
        paint_paths(window, &self.paints);
    }

    pub(crate) fn fill(&self) -> Background {
        self.fill
    }
}

impl GraphDynamicGeometry {
    pub(crate) fn paint(&self, window: &mut Window) {
        paint_paths(window, &self.paints);
    }

    /// Paint the cached tessellated curves translated by `offset` without
    /// rebuilding splines or re-running tessellation (the vendored gpui
    /// `Path::translate` shifts already-built triangles).
    pub(crate) fn paint_translated(&self, window: &mut Window, offset: Point<Pixels>) {
        for paint in &self.paints {
            paint.paint_translated(window, offset);
        }
    }
}

impl GraphGeometry {
    pub(crate) fn paint(&self, window: &mut Window) {
        self.static_geometry.paint(window);
        self.dynamic_geometry.paint(window);
    }
}

fn paint_paths(window: &mut Window, paints: &[GraphPaint]) {
    for paint in paints {
        window.paint_path(paint.path.clone(), paint.color);
    }
}

impl GraphPaint {
    fn paint_translated(&self, window: &mut Window, offset: Point<Pixels>) {
        window.paint_path(self.path.translate(offset), self.color);
    }
}

/// Build the immutable graph scene geometry once for a given bounds/data
/// projection. The grid and fill style are static for a bounds/theme/options
/// key; the curve and area paths remain data-dependent so the scene cache can
/// rebuild only those paths when a telemetry revision advances.
pub(crate) fn build_graph_geometry(
    bounds: Bounds<Pixels>,
    samples: &[f32],
    base: Rgba,
    opts: GraphOpts,
) -> GraphGeometry {
    let static_geometry = build_graph_static_geometry(bounds, base, opts);
    let dynamic_geometry = build_graph_dynamic_geometry(
        bounds,
        samples,
        base,
        opts,
        static_geometry.fill(),
        GraphXMapping::Normal,
    );
    GraphGeometry {
        static_geometry,
        dynamic_geometry,
    }
}

/// Which x-axis mapping a dynamic scene was built with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GraphXMapping {
    /// The settled window mapping used by non-sliding graphs (`sample_x`).
    Normal,
    /// The Mission Center slide base: `capacity + 1` samples on the settled
    /// slot grid, ready to be translated left by the animation progress.
    Slide,
}

/// Build the bounds/theme-dependent portion of a graph scene: grid rules,
/// reference rules, and the fill background used by every finite run.
pub(crate) fn build_graph_static_geometry(
    bounds: Bounds<Pixels>,
    base: Rgba,
    opts: GraphOpts,
) -> GraphStaticGeometry {
    let left = bounds.origin.x;
    let top = bounds.origin.y;
    let w = bounds.size.width;
    let h = bounds.size.height;
    let right = left + w;
    let bottom = top + h;
    let static_count = opts
        .vlines
        .saturating_sub(1)
        .saturating_add(opts.hlines.saturating_sub(1))
        .saturating_add(if opts.ref_lines { 4 } else { 0 });
    let mut paints = Vec::with_capacity(static_count);

    // Faint grid.
    let grid = Rgba {
        a: opts.grid_alpha,
        ..base
    };
    if opts.vlines > 1 {
        for i in 1..opts.vlines {
            let x = left + w * (i as f32 / opts.vlines as f32);
            if let Some(p) = stroke_path(&[point(x, top), point(x, bottom)], opts.stroke_width) {
                paints.push(GraphPaint {
                    path: p,
                    color: grid.into(),
                });
            }
        }
    }
    if opts.hlines > 1 {
        for i in 1..opts.hlines {
            let y = top + h * (i as f32 / opts.hlines as f32);
            if let Some(p) = stroke_path(&[point(left, y), point(right, y)], opts.stroke_width) {
                paints.push(GraphPaint {
                    path: p,
                    color: grid.into(),
                });
            }
        }
    }

    // Opt-in: Mission-Center-style emphasized reference rules at 25/50/75/100%
    // of `max`. Slightly stronger than the faint base grid above, drawn in a
    // low-contrast tint of `base`. Default OFF → no visual change.
    if opts.ref_lines {
        let rl = Rgba {
            a: (opts.grid_alpha * 2.0).min(0.6),
            ..base
        };
        for frac in [0.25_f32, 0.5, 0.75, 1.0] {
            let y = bottom - h * frac;
            if let Some(p) = stroke_path(&[point(left, y), point(right, y)], opts.stroke_width) {
                paints.push(GraphPaint {
                    path: p,
                    color: rl.into(),
                });
            }
        }
    }

    // Fill color: opt-in vertical gradient (line color @ ~0.35 alpha at the top
    // of the area → transparent at the floor), otherwise the flat `fill_alpha`
    // wash that has always been drawn. `linear_gradient(180.0, ...)` points the
    // gradient line downward (stop 0 at the path bbox top, stop 1 at the bottom),
    // per gpui's CSS-aligned angle convention. Shared with the two-series paint
    // path via [`graph_fill_background`].
    let fill = graph_fill_background(base, opts);

    GraphStaticGeometry { paints, fill }
}

/// Build only the data-dependent filled area and curve paths. `fill` comes
/// from the static scene so a new data revision does not recreate the grid or
/// gradient style before tessellating the new curve.
pub(crate) fn build_graph_dynamic_geometry(
    bounds: Bounds<Pixels>,
    samples: &[f32],
    base: Rgba,
    opts: GraphOpts,
    fill: Background,
    x_mapping: GraphXMapping,
) -> GraphDynamicGeometry {
    let left = bounds.origin.x;
    let top = bounds.origin.y;
    let w = bounds.size.width;
    let h = bounds.size.height;
    let right = left + w;
    let bottom = top + h;
    let n = samples.len();
    let mut paints = Vec::with_capacity(2);
    if n == 0 {
        return GraphDynamicGeometry { paints };
    }

    // A non-finite sample represents an explicit provider gap. Split the area
    // into finite runs so missing observations neither become zero nor connect
    // the values on either side with a misleading line.
    for run in finite_sample_runs(samples) {
        let pts = run
            .iter()
            .map(|&(index, value)| {
                let x = match x_mapping {
                    GraphXMapping::Normal => sample_x(left, w, index, n, opts.data_points),
                    GraphXMapping::Slide => sample_x_slide(left, w, index, opts.data_points, 0.0),
                };
                // `.max(1e-6)` guards a caller-supplied `opts.max == 0.0`.
                let yv = (value / opts.max.max(1e-6)).clamp(0.0, 1.0);
                point(x, bottom - h * yv)
            })
            .collect::<Vec<_>>();
        let run_left = pts.first().map_or(left, |point| point.x);
        let run_right = pts.last().map_or(right, |point| point.x);

        // Filled area: top edge follows the polyline (optionally smoothed),
        // then closes square to this finite run's bottom corners.
        if opts.smooth && pts.len() >= 2 {
            if let Some((ctrl_a, ctrl_b)) = catmull_rom_controls(&pts, SMOOTH_TENSION) {
                let mut pb = PathBuilder::fill();
                pb.move_to(pts[0]);
                for i in 0..pts.len() - 1 {
                    pb.cubic_bezier_to(pts[i + 1], ctrl_a[i], ctrl_b[i]);
                }
                pb.line_to(point(run_right, bottom));
                pb.line_to(point(run_left, bottom));
                pb.close();
                if let Ok(path) = pb.build() {
                    paints.push(GraphPaint { path, color: fill });
                }
            }
        } else {
            let mut poly: Vec<Point<Pixels>> = pts.clone();
            poly.push(point(run_right, bottom));
            poly.push(point(run_left, bottom));
            let mut fill_path = PathBuilder::fill();
            fill_path.add_polygon(&poly, true);
            if let Ok(path) = fill_path.build() {
                paints.push(GraphPaint { path, color: fill });
            }
        }

        // Solid stroke on top (optionally smoothed).
        let stroke = if opts.smooth {
            smooth_stroke_path(&pts, opts.stroke_width, SMOOTH_TENSION)
        } else {
            stroke_path(&pts, opts.stroke_width)
        };
        if let Some(path) = stroke {
            paints.push(GraphPaint {
                path,
                color: base.into(),
            });
        }
    }

    GraphDynamicGeometry { paints }
}

pub fn draw_graph(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    samples: &[f32],
    base: Rgba,
    opts: GraphOpts,
) {
    build_graph_geometry(bounds, samples, base, opts).paint(window);
}

pub(crate) fn finite_sample_runs(samples: &[f32]) -> Vec<Vec<(usize, f32)>> {
    let mut runs = Vec::<Vec<(usize, f32)>>::new();
    let mut current = Vec::new();
    for (index, value) in samples.iter().copied().enumerate() {
        if value.is_finite() {
            current.push((index, value));
        } else if !current.is_empty() {
            runs.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs
}

/// Rendering state for a graph whose provider history may still be warming up.
///
/// An empty or one-point window is expected during the first few refreshes,
/// while a non-empty window with no finite observations means the provider
/// explicitly reported an unavailable channel. Keeping those states distinct
/// lets the UI explain a blank canvas without inventing a zero-valued trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GraphSampleState {
    /// No sample slot has been published yet.
    Collecting,
    /// Sample slots exist, but every observation is a provider gap.
    Unavailable,
    /// At least two finite observations are available. Gaps remain holes in
    /// the graph and are intentionally not classified as unavailable.
    Measured,
}

/// Classify a bounded graph window without changing or repairing its samples.
#[must_use]
pub(crate) fn graph_sample_state(samples: &[f32]) -> GraphSampleState {
    if samples.is_empty() {
        GraphSampleState::Collecting
    } else if samples.iter().all(|sample| !sample.is_finite()) {
        GraphSampleState::Unavailable
    } else if samples.iter().filter(|sample| sample.is_finite()).count() < 2 {
        GraphSampleState::Collecting
    } else {
        GraphSampleState::Measured
    }
}

/// Classify a two-series graph window by the UNION of its directions'
/// evidence: a graph whose read direction is measured is NOT unavailable just
/// because the summed lane (or the write direction) holds only gaps. Empty
/// windows stay Collecting; slots with no finite observation on either side
/// stay Unavailable; fewer than two union-finite observations stay Collecting
/// — the single-series rules applied to the combined evidence.
#[must_use]
pub(crate) fn graph_dual_sample_state(primary: &[f32], secondary: &[f32]) -> GraphSampleState {
    if primary.is_empty() && secondary.is_empty() {
        return GraphSampleState::Collecting;
    }
    let finite = primary
        .iter()
        .chain(secondary)
        .filter(|sample| sample.is_finite())
        .count();
    match finite {
        0 => GraphSampleState::Unavailable,
        1 => GraphSampleState::Collecting,
        _ => GraphSampleState::Measured,
    }
}

fn stroke_path(pts: &[Point<Pixels>], width: f32) -> Option<Path<Pixels>> {
    if pts.is_empty() {
        return None;
    }
    let mut pb = PathBuilder::stroke(px(width));
    for (i, p) in pts.iter().enumerate() {
        if i == 0 {
            pb.move_to(*p);
        } else {
            pb.line_to(*p);
        }
    }
    pb.build().ok()
}

/// Cubic-Bézier control-point pairs for a smoothed polyline: `(ctrl_a, ctrl_b)`
/// where index `i` holds the two control points for segment `pts[i] -> pts[i+1]`.
/// Factored into a `type` alias to keep [`catmull_rom_controls`]'s signature
/// under clippy's `type_complexity` threshold.
type SmoothControls = (Vec<Point<Pixels>>, Vec<Point<Pixels>>);

/// Cubic-Bézier control points for a (uniform) Catmull-Rom spline through `pts`,
/// scaled by `tension` (see [`SMOOTH_TENSION`]). Returns `(ctrl_a, ctrl_b)` where
/// `(ctrl_a[i], ctrl_b[i])` are the two control points for the segment
/// `pts[i] -> pts[i+1]`. End segments are clamped (phantom neighbors = the
/// nearest endpoint), which keeps the spline anchored to the first/last sample
/// instead of overshooting. Returns `None` for fewer than 2 points.
///
/// Math is done in raw `f32` (via `f32::from(Pixels)`) and rebuilt with `px(..)`
/// to stay clear of `Pixels`-by-`f32` operator ambiguities; this mirrors the
/// arithmetic style already used in [`sample_at_cursor_x`].
fn catmull_rom_controls(pts: &[Point<Pixels>], tension: f32) -> Option<SmoothControls> {
    let n = pts.len();
    if n < 2 {
        return None;
    }
    let f = |p: Point<Pixels>| (f32::from(p.x), f32::from(p.y));
    let mut ctrl_a = Vec::with_capacity(n - 1);
    let mut ctrl_b = Vec::with_capacity(n - 1);
    for i in 0..n - 1 {
        let (x1, y1) = f(pts[i]);
        let (x2, y2) = f(pts[i + 1]);
        let (x0, y0) = if i == 0 { (x1, y1) } else { f(pts[i - 1]) };
        let (x3, y3) = if i + 2 < n { f(pts[i + 2]) } else { (x2, y2) };
        let ca = point(
            px(x1 + (x2 - x0) / 6.0 * tension),
            px(y1 + (y2 - y0) / 6.0 * tension),
        );
        let cb = point(
            px(x2 - (x3 - x1) / 6.0 * tension),
            px(y2 - (y3 - y1) / 6.0 * tension),
        );
        ctrl_a.push(ca);
        ctrl_b.push(cb);
    }
    Some((ctrl_a, ctrl_b))
}

/// Like [`stroke_path`] but routes the polyline through a tension-scaled
/// Catmull-Rom spline (cubic-Bézier segments). Falls back to [`stroke_path`] for
/// fewer than 2 points so the degenerate cases render exactly as the baseline.
fn smooth_stroke_path(pts: &[Point<Pixels>], width: f32, tension: f32) -> Option<Path<Pixels>> {
    let n = pts.len();
    if n < 2 {
        return stroke_path(pts, width);
    }
    let (ctrl_a, ctrl_b) = catmull_rom_controls(pts, tension)?;
    let mut pb = PathBuilder::stroke(px(width));
    pb.move_to(pts[0]);
    for i in 0..n - 1 {
        pb.cubic_bezier_to(pts[i + 1], ctrl_a[i], ctrl_b[i]);
    }
    pb.build().ok()
}

// Paint of the `GraphOpts::value_badge` pill and its shaped-text cache moved
// to `scene_cache::paint_graph_scene`: the cache now lives across frames
// keyed by the samples' identity and canvas bounds, so hover and resize
// repaints replay the shaped line instead of re-running the text system (see
// the module docs there).
fn latest_finite_sample(samples: &[f32]) -> Option<f32> {
    samples.last().copied().filter(|sample| sample.is_finite())
}

/// A gpui element that fills its parent and paints a graph from `samples`.
///
/// Samples convert from `Vec<f32>` or a shared `Rc<[f32]>`; callers that reuse
/// one projection across many canvases (the per-core CPU grid) pass the `Rc`
/// so each cell pays an `Rc` clone instead of a full history clone. The built
/// geometry is cached across frames by the scene store (the private
/// `graph::scene_cache` module): a repaint with unchanged samples and bounds
/// replays the cached paths instead of re-running the spline + tessellation
/// pass.
pub(crate) fn graph_element(
    id: impl Into<ElementId>,
    samples: impl Into<Rc<[f32]>>,
    base: Rgba,
    opts: GraphOpts,
    cache: GraphCacheHandle,
) -> AnyElement {
    let id: ElementId = id.into();
    let samples = cache
        .borrow_mut()
        .latest_samples(samples.into(), opts.data_points, opts.sliding);
    let sliding = opts.sliding && graph_slide_supported(&samples, opts.data_points);
    let slide_key = id.clone();
    let paint_cache = cache;
    let graph = canvas(
        |_bounds, _window, _cx| (),
        move |bounds, _t, window, cx| {
            let mut cache = paint_cache.borrow_mut();
            let slide_started_at = sliding.then(|| {
                cache
                    .slides_mut()
                    .timing_for(&slide_key, &samples, Instant::now())
            });
            let progress =
                slide_started_at.map_or(1.0, |started_at| slide_progress(started_at, window));
            let offset = if sliding {
                let slot = graph_slide_spacing(bounds, opts.data_points);
                px(-f32::from(slot) * progress)
            } else {
                px(0.0)
            };
            scene_cache::paint_graph_scene(
                cache.scenes_mut(),
                window,
                cx,
                bounds,
                &samples,
                base,
                opts,
                offset,
            );
        },
    )
    .size_full();
    div()
        .size_full()
        .overflow_hidden()
        .child(graph)
        .into_any_element()
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_graph_tests.rs"]
mod tests;
