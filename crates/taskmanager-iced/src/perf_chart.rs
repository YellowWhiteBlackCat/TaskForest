//! Minimal line/area chart for the Performance page, built on iced 0.14's
//! `Canvas` + `Program` (iced has no built-in chart widget).
//!
//! Each series is a stroked polyline of its ring-buffer samples normalized
//! onto the canvas frame, with a Mission-Center-style area fill: a vertical
//! gradient from the series color at ~0.35 alpha on the top edge down to fully
//! transparent at the baseline (built on iced's public
//! [`canvas::gradient::Linear`] + `Fill::from`). The CPU/memory chart is also
//! hover-interactive: [`HoverState`] lives in the widget-tree `Program::State`
//! (persistent across frames), cursor motion updates it through
//! [`Program::update`], and the draw pass renders a reference line plus a
//! value pill at the hovered sample.
//!
//! The point projection ([`series_point_runs`]) and the hover index mapping
//! ([`hovered_index`]) are pure functions so the geometry math is unit-tested
//! without a live renderer; [`PerfChart::draw`] only feeds those points into
//! an iced `Frame`. The window→geometry mapping itself is unified in
//! [`WindowSlots`] (right-anchored capacity slots, GPUI `sample_x` parity):
//! drawing x, the windowed projection ([`series_point_runs_windowed`]), and
//! hover resolution ([`WindowSlots::index_at`]) all derive from the same slot
//! denominator, so a tooltip can never disagree with a drawn sample. Grid
//! counts/alpha and series stroke width are parameterized through
//! [`ChartOpts`] (GPUI `GraphOpts` grid semantics) with defaults pinned to
//! the legacy look.
//!
//! Cross-frame geometry reuse: iced repaints at vsync (~60 Hz) while the series
//! advance only ~1 Hz and the cursor moves independently. The state therefore
//! holds TWO physically separate [`canvas::Cache`]s (the round-1
//! `process_sparkline` pattern, extended to a hover chart): the DATA cache
//! stores the grid + two series (cleared only when the data fingerprint
//! changes — new sample / window shift / smooth toggle), and the OVERLAY cache
//! stores the hover readout pill (cleared when the hover index OR the data
//! fingerprint changes). Cursor motion therefore never busts the expensive data
//! geometry, and a moved pill never shows a stale reading.

use std::cell::RefCell;
use std::rc::Rc;

use iced::mouse;
use iced::widget::canvas::{self, Cache, Geometry, Path, Stroke};
use iced::{Color, Pixels, Point, Rectangle, Size};

use taskmanager_application::i18n::t;

use crate::app::Message;

mod program;

/// Stroke width for each series polyline.
const SERIES_STROKE_WIDTH: f32 = 1.6;

/// Alpha of the area-fill gradient at the top edge, directly under the
/// polyline (Mission-Center-style fade: category color at ~0.35 → transparent).
const AREA_FILL_TOP_ALPHA: f32 = 0.35;

/// Alpha of the area-fill gradient at the bottom edge (fully transparent).
const AREA_FILL_BOTTOM_ALPHA: f32 = 0.0;

/// The midpoint offset of the area-fill gradient (between the top and bottom
/// stops) so the fade bends like Mission Center's rather than a hard ramp.
const AREA_FILL_MID_OFFSET: f32 = 0.55;

/// The hover readout pill's token-derived colors: the surface fill the pill
/// sits on and the foreground its values are drawn in. Bundled so the chart
/// factories stay under clippy's argument budget (same pattern as
/// `device_chart::GraphPrefs`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ReadoutColors {
    pub bg: Color,
    pub fg: Color,
}

/// Stable identity of one immutable series snapshot. Holding an `Rc` clone in
/// canvas state makes pointer identity a safe generation key: while the old
/// snapshot is retained its allocation cannot be reused, and a new history
/// revision receives a new allocation even when length and tail value happen
/// to match.
#[derive(Clone, Default, Debug)]
pub(crate) struct SeriesGeneration(Option<Rc<[f32]>>);

impl SeriesGeneration {
    #[must_use]
    pub(crate) fn new(samples: &Rc<[f32]>) -> Self {
        Self(Some(Rc::clone(samples)))
    }
}

impl PartialEq for SeriesGeneration {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(left), Some(right)) => Rc::ptr_eq(left, right),
            (None, None) => true,
            _ => false,
        }
    }
}

/// A two-series CPU% / memory% line+area chart. Built fresh each render from
/// the shared shell `LiveGraphHistory` series samples (G-02: the retired
/// renderer-local `PerfHistory` ring's replacement data source) and the
/// resolved theme colors; produces no application messages (hover redraws are
/// requested through canvas actions, never through the app's message loop).
/// When `smooth` is set the polylines pass through a Catmull-Rom spline
/// instead of straight segments (GPUI's smooth-graphs preference).
pub(crate) struct PerfChart {
    pub cpu: Rc<[f32]>,
    pub memory: Rc<[f32]>,
    pub cpu_color: Color,
    pub memory_color: Color,
    pub grid_color: Color,
    pub readout: ReadoutColors,
    pub smooth: bool,
}

impl PerfChart {
    /// Build the chart program from one snapshot of the two series plus the
    /// token-derived colors the polylines and the hover readout should wear.
    pub(crate) fn new<C, M>(
        cpu: C,
        memory: M,
        cpu_color: Color,
        memory_color: Color,
        grid_color: Color,
        readout: ReadoutColors,
        smooth: bool,
    ) -> Self
    where
        C: Into<Rc<[f32]>>,
        M: Into<Rc<[f32]>>,
    {
        Self {
            cpu: cpu.into(),
            memory: memory.into(),
            cpu_color,
            memory_color,
            grid_color,
            readout,
            smooth,
        }
    }

    /// The data fingerprint identifying the grid + two-series geometry this
    /// program would draw for its current data. Pure so the cross-frame
    /// data-cache-clear gate is unit-tested without a renderer (see
    /// [`PerfChartDataFingerprint`]).
    #[must_use]
    pub(crate) fn fingerprint(&self) -> PerfChartDataFingerprint {
        PerfChartDataFingerprint::from_series(&self.cpu, &self.memory, self.smooth)
    }
}

/// The cached identity of the chart's DATA geometry: both immutable snapshot
/// generations plus smoothing. Retained generations detect a shifted window
/// even when length/tail agree, without hashing every frame. Colors/theme
/// tokens are NOT part of the fingerprint (a theme switch
/// is rare and one stale-color frame is acceptable — matches round-1
/// `process_sparkline`).
#[derive(Clone, Default, PartialEq, Debug)]
pub(crate) struct PerfChartDataFingerprint {
    cpu: SeriesGeneration,
    memory: SeriesGeneration,
    smooth: bool,
}

impl PerfChartDataFingerprint {
    /// Build the data fingerprint from both generations plus smoothing.
    #[must_use]
    fn from_series(cpu: &Rc<[f32]>, memory: &Rc<[f32]>, smooth: bool) -> Self {
        Self {
            cpu: SeriesGeneration::new(cpu),
            memory: SeriesGeneration::new(memory),
            smooth,
        }
    }
}

/// The cached identity of the chart's OVERLAY geometry (hover readout pill):
/// the hovered sample index plus the data fingerprint. The pill text is read
/// from the samples at draw time, so the overlay must rebuild when EITHER the
/// hover moves OR the underlying data ticks (otherwise a moved cursor would
/// show a stale reading). `None` index = no hover (empty overlay).
#[derive(Clone, Default, PartialEq, Debug)]
struct PerfChartOverlayFingerprint {
    hover_index: Option<usize>,
    data: PerfChartDataFingerprint,
}

/// The persistent per-canvas state for the hover-interactive CPU/memory chart:
/// the [`HoverState`] (updated through [`Program::update`] on cursor motion),
/// plus TWO physically separate [`canvas::Cache`]s — the data cache (grid +
/// series, cleared only on a data-fingerprint change) and the overlay cache
/// (hover readout pill, cleared on a hover-index OR data-fingerprint change).
/// `Default`-derivable so iced can seed one when the canvas node first appears.
#[derive(Default)]
pub(crate) struct PerfChartState {
    /// The hovered sample index, updated through `update` on cursor motion.
    pub(crate) hover: HoverState,
    data_cache: Cache,
    overlay_cache: Cache,
    data_fingerprint: RefCell<PerfChartDataFingerprint>,
    overlay_fingerprint: RefCell<PerfChartOverlayFingerprint>,
}

/// Hover state for the chart canvas, stored in the widget-tree `Program::State`
/// so it survives the per-frame rebuild of the [`PerfChart`] struct (iced
/// persists the canvas state across frames while the canvas node is stable).
/// Holds only the hovered sample index; the values are re-read from the
/// current samples at draw time, so a shifted ring buffer never shows a stale
/// reading.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HoverState {
    pub(crate) index: Option<usize>,
}

/// Map a cursor x onto the nearest sample index, mirroring the
/// [`series_point_runs`] projection (samples spread evenly from left to right,
/// oldest → newest). `None` when the cursor is outside the frame, the frame
/// has no width, or the window holds fewer than two samples (nothing to
/// plot/hover — the honest too-few-samples state). Pure so the mapping is
/// unit-tested without a renderer.
#[must_use]
pub(crate) fn hovered_index(cursor_x: f32, width: f32, sample_count: usize) -> Option<usize> {
    WindowSlots::spread(sample_count, width).index_at(cursor_x)
}

/// Return the x-coordinate for one chronological sample.
///
/// History accessors guarantee oldest→newest order. Keeping this projection in
/// one named helper makes the direction explicit and lets every chart's hover
/// overlay use the same left-to-right contract instead of re-implementing an
/// easy-to-reverse `step * index` expression. Implemented as the full-window
/// specialization of [`WindowSlots`] so the legacy spread and the capacity
/// windowed mapping cannot drift apart.
#[must_use]
pub(crate) fn sample_x(index: usize, sample_count: usize, width: f32) -> f32 {
    WindowSlots::spread(sample_count, width).x(index)
}

/// The ONE window→geometry mapping every chart shares (GPUI `sample_x` /
/// `sample_index_at_cursor_x` parity): the newest sample pins to the right
/// edge and every sample owns a fixed slot of the window — a growing window
/// (`sample_count < capacity`) therefore grows a short, evenly-paced curve
/// from the right edge leftward instead of stretching two samples across the
/// full width. Both directions — the drawing x ([`WindowSlots::x`]) and the
/// hover index resolution ([`WindowSlots::index_at`]) — derive from the same
/// denominator, so a hover readout can never disagree with a drawn sample.
///
/// The full-window form ([`WindowSlots::spread`], capacity == sample count) is
/// exactly the legacy [`sample_x`]/[`hovered_index`] spread, so charts that
/// keep it render pixel-identical to before.
///
/// Fewer than two samples carry no honest span: `x` collapses to `0.0` and
/// `index_at` is `None` (the too-few-samples state every chart refuses to
/// fabricate a line for). Pure so the slot math is unit-tested headlessly.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WindowSlots {
    sample_count: usize,
    capacity: usize,
    width: f32,
}

impl WindowSlots {
    /// Build the slot grid for a window of `sample_count` samples inside a
    /// `capacity`-wide frame (`width` pixels). A capacity below the sample
    /// count is safe: the denominator is their maximum, so an over-full window
    /// degrades to the full-span spread rather than dividing by zero.
    #[must_use]
    pub(crate) fn new(sample_count: usize, capacity: usize, width: f32) -> Self {
        Self {
            sample_count,
            capacity,
            width,
        }
    }

    /// The full-window form: every sample spreads evenly across the whole
    /// width (the legacy [`sample_x`] projection — no right-anchored growth).
    #[must_use]
    pub(crate) fn spread(sample_count: usize, width: f32) -> Self {
        Self::new(sample_count, sample_count, width)
    }

    fn denominator(&self) -> f32 {
        self.sample_count
            .saturating_sub(1)
            .max(self.capacity.saturating_sub(1))
            .max(1) as f32
    }

    /// The x-coordinate of the sample at `index` (oldest→newest slots,
    /// newest pinned at `width`). Indices beyond the window clamp to the
    /// newest slot; a window with fewer than two samples has no span (`0.0`).
    #[must_use]
    pub(crate) fn x(&self, index: usize) -> f32 {
        if self.sample_count < 2 {
            return 0.0;
        }
        let width = if self.width.is_finite() {
            self.width.max(0.0)
        } else {
            0.0
        };
        let from_right = (self.sample_count - 1 - index.min(self.sample_count - 1)) as f32;
        width * (1.0 - from_right / self.denominator())
    }

    /// The inverse of [`WindowSlots::x`]: map a frame-relative cursor x onto
    /// the nearest sample slot. `None` outside the frame, for a non-positive
    /// or non-finite width, or when the window holds fewer than two samples
    /// (nothing to hover — the honest too-few-samples state). Uses the same
    /// denominator as `x`, so the resolved index always lands on the slot the
    /// cursor is actually over, even in a partial right-anchored window.
    #[must_use]
    pub(crate) fn index_at(&self, cursor_x: f32) -> Option<usize> {
        if self.sample_count < 2
            || !cursor_x.is_finite()
            || !self.width.is_finite()
            || self.width <= 0.0
            || cursor_x < 0.0
            || cursor_x >= self.width
        {
            return None;
        }
        let relative = (cursor_x / self.width).clamp(0.0, 1.0);
        let index = (self.sample_count as f32 - 1.0 - (1.0 - relative) * self.denominator())
            .round()
            .clamp(0.0, (self.sample_count - 1) as f32);
        Some(index as usize)
    }
}

/// The three color stops of the vertical area-fill gradient: the series color
/// at [`AREA_FILL_TOP_ALPHA`] on the top edge, a translucent midpoint, and
/// fully transparent at the baseline. Ascending offsets (0.0 → 1.0) with
/// strictly decreasing alpha — the Mission-Center fade. Pure so the alpha
/// monotonicity is unit-tested headlessly.
#[must_use]
pub(crate) fn area_gradient_stops(color: Color) -> [(f32, Color); 3] {
    let top = color.scale_alpha(AREA_FILL_TOP_ALPHA);
    let bottom = color.scale_alpha(AREA_FILL_BOTTOM_ALPHA);
    let mid_alpha = (AREA_FILL_TOP_ALPHA + AREA_FILL_BOTTOM_ALPHA) / 2.0;
    let mid = color.scale_alpha(mid_alpha);
    [(0.0, top), (AREA_FILL_MID_OFFSET, mid), (1.0, bottom)]
}

/// Build the vertical area-fill gradient for one series: iced's public
/// [`canvas::gradient::Linear`] from the frame top (`y = 0`) to the baseline
/// (`y = height`) with the [`area_gradient_stops`] ramp. The returned value
/// converts into a canvas `Fill` (`Fill: From<Linear>`), so callers pass it
/// straight to `Frame::fill`. Pure — the geometry and stops are unit-tested
/// without a renderer.
#[must_use]
pub(crate) fn vertical_area_gradient(color: Color, height: f32) -> canvas::gradient::Linear {
    let mut gradient = canvas::gradient::Linear::new(Point::new(0.0, 0.0), Point::new(0.0, height));
    for (offset, stop_color) in area_gradient_stops(color) {
        gradient = gradient.add_stop(offset, stop_color);
    }
    gradient
}

/// Tunable chart grid/glyph parameters — the iced counterpart of GPUI's
/// `GraphOpts` grid knobs (`grid_alpha` / `hlines` / `vlines` / stroke
/// width), with [`ChartOpts::DEFAULT`] pinned to the exact legacy look so
/// parameterizing the grid changes no existing chart's pixels. Counts are
/// rule counts (the legacy grid draws 4 horizontal and 6 vertical rules);
/// colors stay token-derived — only the counts and the alpha multiplier of
/// the caller's border token are configurable, so no literal color can enter
/// the grid.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChartOpts {
    /// Horizontal grid rule count (rules divide the frame height evenly).
    pub hlines: usize,
    /// Vertical grid rule count (rules divide the frame width evenly).
    pub vlines: usize,
    /// Alpha multiplier applied to the token-derived grid color.
    pub grid_alpha: f32,
    /// Stroke width of the series polylines.
    pub stroke_width: f32,
}

impl ChartOpts {
    /// The legacy look as a const, so chart modules can name it in constants:
    /// the quarter grid (4 horizontal rules), six vertical rules, border
    /// token at 0.48 alpha, and the standard series stroke.
    pub(crate) const DEFAULT: Self = Self {
        hlines: 4,
        vlines: 6,
        grid_alpha: 0.48,
        stroke_width: SERIES_STROKE_WIDTH,
    };
}

impl Default for ChartOpts {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Draw the same quiet reference grid that makes GPUI's graph cards readable
/// ([`ChartOpts`] semantics): callers provide the border token resolved by
/// their frontend, so no toolkit or literal color leaks into the chart
/// geometry. `hlines` horizontal rules at `i / (hlines + 1)` of the height
/// and `vlines` vertical rules at `i / (vlines + 1)` of the width, each
/// stroked once with the token color at `grid_alpha`. A zero rule count
/// draws no rules of that orientation.
pub(crate) fn draw_grid_opts(
    frame: &mut canvas::Frame<iced::Renderer>,
    size: Size,
    color: Color,
    opts: ChartOpts,
) {
    let stroke = Stroke::default()
        .with_width(1.0)
        .with_color(color.scale_alpha(opts.grid_alpha));
    for index in 1..=opts.hlines {
        let y = size.height * index as f32 / (opts.hlines + 1) as f32;
        frame.stroke(
            &Path::line(Point::new(0.0, y), Point::new(size.width, y)),
            stroke,
        );
    }
    for index in 1..=opts.vlines {
        let x = size.width * index as f32 / (opts.vlines + 1) as f32;
        frame.stroke(
            &Path::line(Point::new(x, 0.0), Point::new(x, size.height)),
            stroke,
        );
    }
}

/// The y-axis tick ladder: `[max, max / 2, 0]` — the three labels that make a
/// magnitude graph readable without crowding a mini-chart. Empty for a
/// non-positive or non-finite `max` (the honest idle window carries no scale
/// to read). Pure so the ladder is unit-tested headlessly.
#[must_use]
pub(crate) fn y_axis_tick_values(max: f32) -> Vec<f32> {
    if !max.is_finite() || max <= 0.0 {
        return Vec::new();
    }
    vec![max, max / 2.0, 0.0]
}

/// Draw the y-axis tick labels for one ladder (see [`y_axis_tick_values`]):
/// each value's text (formatted by the caller's injected unit formatter — the
/// MB/s spelling belongs to the call site, never the chart) sits at the
/// height its value maps to under [`scaled_y`], in a token-derived muted
/// color. Labels are clamped inside the frame so the 0 and max ticks never
/// half-clip past the edges.
pub(crate) fn draw_y_axis_ticks(
    frame: &mut canvas::Frame<iced::Renderer>,
    size: Size,
    ticks: &[f32],
    max: f32,
    format_value: impl Fn(f32) -> String,
    color: Color,
) {
    if !max.is_finite() || max <= 0.0 {
        return;
    }
    for &value in ticks {
        let fraction = (value / max).clamp(0.0, 1.0);
        let y = (size.height * (1.0 - fraction)).clamp(5.0, (size.height - 5.0).max(5.0));
        frame.fill_text(canvas::Text {
            content: format_value(value),
            position: Point::new(3.0, y),
            color,
            size: Pixels(9.0),
            align_y: iced::alignment::Vertical::Center,
            ..canvas::Text::default()
        });
    }
}

/// The GPUI-parity hover sample mark: one horizontal reference rule through
/// the hovered sample's y plus a small snap dot at `(x, y)` in the series'
/// own color. Paired with the vertical rule the readout pill already draws,
/// this completes the crosshair — vertical and horizontal rules and the dot
/// all land on the SAME slot-mapped sample the tooltip reads, so the
/// crosshair can never point beside the value it reports (GPUI
/// `draw_graph_crosshair` semantics, snapped rather than free-following).
pub(crate) fn draw_hover_sample_mark(
    frame: &mut canvas::Frame<iced::Renderer>,
    size: Size,
    x: f32,
    y: f32,
    dot_color: Color,
    rule_color: Color,
) {
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    frame.stroke(
        &Path::line(Point::new(0.0, y), Point::new(size.width, y)),
        Stroke::default()
            .with_width(1.0)
            .with_color(rule_color.scale_alpha(0.7)),
    );
    frame.fill(&Path::circle(Point::new(x, y), 2.5), dot_color);
}

/// Stroke the polyline and fill its area for one series. A no-op for buffers
/// with fewer than two samples — the honest too-few-samples state, never a
/// fabricated single-segment line. The area fill is the vertical
/// Mission-Center gradient ([`vertical_area_gradient`]); when `smooth` is set
/// the stroke and the fill's top edge run through a Catmull-Rom spline (GPUI
/// parity).
fn draw_series(
    frame: &mut canvas::Frame<iced::Renderer>,
    samples: &[f32],
    size: Size,
    color: Color,
    smooth: bool,
) {
    for points in series_point_runs(samples, size) {
        if points.len() < 2 {
            continue;
        }
        if smooth && points.len() >= 3 {
            if let Some(area) = smooth_area_path(&points, size.height) {
                frame.fill(&area, vertical_area_gradient(color, size.height));
            }
            if let Some(line) = smooth_line_path(&points) {
                frame.stroke(
                    &line,
                    Stroke::default()
                        .with_width(SERIES_STROKE_WIDTH)
                        .with_color(color),
                );
            }
            continue;
        }
        if let Some(area) = area_path(&points, size.height) {
            frame.fill(&area, vertical_area_gradient(color, size.height));
        }
        if let Some(line) = line_path(&points) {
            frame.stroke(
                &line,
                Stroke::default()
                    .with_width(SERIES_STROKE_WIDTH)
                    .with_color(color),
            );
        }
    }
}

/// The hover readout geometry shared by every hover-interactive chart: a
/// vertical reference line through the hovered sample's x plus a small rounded
/// value pill pinned near the top of the frame. `content` is the chart's own
/// pre-formatted readout label (`None` still draws the reference line — the
/// honest "position, no value" state — but no pill); the pill is clamped
/// inside the frame so an edge sample never draws its label off-canvas. The
/// pill's width is a fixed per-glyph estimate (the canvas has no text
/// measurement without a renderer) — a layout approximation, not a measured
/// glyph width.
pub(crate) fn draw_readout_pill(
    frame: &mut canvas::Frame<iced::Renderer>,
    size: Size,
    x: f32,
    content: Option<&str>,
    grid_color: Color,
    readout: ReadoutColors,
) {
    frame.stroke(
        &Path::line(Point::new(x, 0.0), Point::new(x, size.height)),
        Stroke::default()
            .with_width(1.0)
            .with_color(grid_color.scale_alpha(0.7)),
    );

    let Some(content) = content else {
        return;
    };
    let font_size = 11.0;
    let text_width = content.chars().count() as f32 * font_size * 0.6;
    let pill_width = text_width + 12.0;
    let pill_height = 18.0;
    let pill_x = (x - pill_width / 2.0).clamp(2.0, (size.width - pill_width - 2.0).max(2.0));
    let pill_y = 4.0;
    let pill = Path::rounded_rectangle(
        Point::new(pill_x, pill_y),
        Size::new(pill_width, pill_height),
        iced::border::Radius::from(8.0),
    );
    frame.fill(&pill, readout.bg);
    frame.stroke(
        &pill,
        Stroke::default()
            .with_width(1.0)
            .with_color(grid_color.scale_alpha(0.6)),
    );
    frame.fill_text(canvas::Text {
        content: content.to_string(),
        position: Point::new(pill_x + pill_width / 2.0, pill_y + pill_height / 2.0),
        color: readout.fg,
        size: Pixels(font_size),
        align_x: iced::advanced::text::Alignment::Center,
        align_y: iced::alignment::Vertical::Center,
        ..canvas::Text::default()
    });
}

/// The CPU/memory chart's hover readout geometry: the shared
/// [`draw_readout_pill`] at the hovered sample's x, plus a GPUI-parity
/// [`draw_hover_sample_mark`] per series (horizontal rule + snap dot on the
/// hovered sample, in the series' own color), labeled by [`readout_text`].
/// Only series that actually have a value at the index contribute a mark or
/// label (partial buffers stay honest).
fn draw_hover_readout(
    frame: &mut canvas::Frame<iced::Renderer>,
    size: Size,
    index: usize,
    series: &[(&[f32], Color); 2],
    grid_color: Color,
    readout: ReadoutColors,
) {
    let [(cpu, cpu_color), (memory, memory_color)] = series;
    let sample_count = cpu.len().max(memory.len());
    let x = sample_x(index, sample_count, size.width);
    for (samples, color) in [(cpu, *cpu_color), (memory, *memory_color)] {
        if let Some(&value) = samples.get(index).filter(|value| value.is_finite()) {
            draw_hover_sample_mark(
                frame,
                size,
                x,
                scaled_y(value, 100.0, size.height),
                color,
                grid_color,
            );
        }
    }
    draw_readout_pill(
        frame,
        size,
        x,
        readout_text(cpu, memory, index).as_deref(),
        grid_color,
        readout,
    );
}

/// The value pill's text at one sample index: each series that actually holds
/// a value at the index contributes "CPU 43%" / "Mem 61%", joined by " · ".
/// `None` when neither series has a value there (nothing to read out — the
/// honest partial-buffer state). Pure so the label composition is unit-tested
/// headlessly.
#[must_use]
pub(crate) fn readout_text(cpu: &[f32], memory: &[f32], index: usize) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(value) = cpu.get(index).filter(|value| value.is_finite()) {
        parts.push(format!("{} {:.0}%", t("common.cpu"), value));
    }
    if let Some(value) = memory.get(index).filter(|value| value.is_finite()) {
        parts.push(format!("{} {:.0}%", t("common.memory"), value));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

/// The Catmull-Rom tension for [`smooth_line_path`] (matches GPUI's
/// `SMOOTH_TENSION` — the curve passes through the sample points).
const SMOOTH_TENSION: f32 = 1.0 / 6.0;

/// Build the smooth open path through `points`: each segment becomes a cubic
/// Bezier whose control points come from the neighbouring samples
/// (Catmull-Rom → Bezier conversion). Returns `None` for fewer than three
/// vertices (the spline needs a neighbour on both sides of at least one
/// segment). Pure function so the spline geometry is unit-tested headlessly.
#[must_use]
pub(crate) fn smooth_line_path(points: &[Point]) -> Option<Path> {
    if points.len() < 3 || !points_are_finite(points) {
        return None;
    }
    Some(Path::new(|builder| {
        builder.move_to(points[0]);
        for index in 0..points.len() - 1 {
            let previous = points[index.saturating_sub(1)];
            let start = points[index];
            let end = points[index + 1];
            let next = points.get(index + 2).copied().unwrap_or(end);
            let ctrl_a = Point::new(
                start.x + (end.x - previous.x) * SMOOTH_TENSION,
                start.y + (end.y - previous.y) * SMOOTH_TENSION,
            );
            let ctrl_b = Point::new(
                end.x - (next.x - start.x) * SMOOTH_TENSION,
                end.y - (next.y - start.y) * SMOOTH_TENSION,
            );
            builder.bezier_curve_to(end, ctrl_a, ctrl_b);
        }
    }))
}

/// Build the closed smooth area polygon: the spline down the top edge,
/// closing square to this run's baseline corners. Returns `None` for fewer
/// than three vertices.
#[must_use]
pub(crate) fn smooth_area_path(points: &[Point], baseline_y: f32) -> Option<Path> {
    if points.len() < 3 || !baseline_y.is_finite() || !points_are_finite(points) {
        return None;
    }
    let first = points[0];
    let last = points[points.len() - 1];
    Some(Path::new(|builder| {
        builder.move_to(first);
        for index in 0..points.len() - 1 {
            let previous = points[index.saturating_sub(1)];
            let start = points[index];
            let end = points[index + 1];
            let next = points.get(index + 2).copied().unwrap_or(end);
            let ctrl_a = Point::new(
                start.x + (end.x - previous.x) * SMOOTH_TENSION,
                start.y + (end.y - previous.y) * SMOOTH_TENSION,
            );
            let ctrl_b = Point::new(
                end.x - (next.x - start.x) * SMOOTH_TENSION,
                end.y - (next.y - start.y) * SMOOTH_TENSION,
            );
            builder.bezier_curve_to(end, ctrl_a, ctrl_b);
        }
        builder.line_to(Point::new(last.x, baseline_y));
        builder.line_to(Point::new(first.x, baseline_y));
        builder.close();
    }))
}

/// Build the open polyline path through `points`. Returns `None` for fewer
/// than two vertices (no strokeable segment). Shared with the trend strip so
/// the two widgets cannot drift apart on polyline construction.
pub(crate) fn line_path(points: &[Point]) -> Option<Path> {
    if points.len() < 2 || !points_are_finite(points) {
        return None;
    }
    let first = points[0];
    Some(Path::new(|builder| {
        builder.move_to(first);
        for point in &points[1..] {
            builder.line_to(*point);
        }
    }))
}

/// Build the closed area polygon: the polyline followed back along the
/// baseline (`y = baseline_y`) so the fill reads as "area under the curve".
/// Returns `None` for fewer than two vertices. Shared with the device headline
/// graph and the per-core cell sparkline so every area fill closes back along
/// the same baseline.
pub(crate) fn area_path(points: &[Point], baseline_y: f32) -> Option<Path> {
    if points.len() < 2 || !baseline_y.is_finite() || !points_are_finite(points) {
        return None;
    }
    let first = points[0];
    let last = points[points.len() - 1];
    Some(Path::new(|builder| {
        builder.move_to(first);
        for point in &points[1..] {
            builder.line_to(*point);
        }
        builder.line_to(Point::new(last.x, baseline_y));
        builder.line_to(Point::new(first.x, baseline_y));
        builder.close();
    }))
}

/// Whether every point is safe to pass to iced/lyon's path builder.
fn points_are_finite(points: &[Point]) -> bool {
    points
        .iter()
        .all(|point| point.x.is_finite() && point.y.is_finite())
}

/// Project `samples` scaled to `max` into finite contiguous runs inside `size`.
///
/// Same geometry as the 0–100 percentage chart, but for series whose unit is
/// not a percentage (e.g. disk/network bytes/sec): each value maps to
/// `value / max` of the frame height. `max <= 0` (or all-zero samples) anchors
/// every point at the baseline — the honest idle state, never a fabricated
/// mid-line.
///
/// The frame origin is the top-left and y grows downward, so `0` maps to the
/// bottom edge (`y = height`) and `max` to the top edge (`y = 0`). Samples
/// spread evenly across the width left→right (oldest→newest); values are
/// clamped to `[0, max]` so an out-of-range reading cannot escape the frame.
///
/// A non-finite sample is an authoritative history gap. It ends the current
/// run while retaining its chronological slot in the x projection; the next
/// finite sample starts a new run at its original x. This prevents both an
/// invalid lyon point and the more subtle lie of filtering gaps and drawing a
/// line or area across missing evidence.
///
/// Empty/all-gap input yields no runs. Edge gaps likewise create no fabricated
/// vertices.
#[must_use]
pub(crate) fn series_point_runs_for(samples: &[f32], size: Size, max: f32) -> Vec<Vec<Point>> {
    series_point_runs_with(samples, size, max, &|index| {
        sample_x(index, samples.len(), size.width)
    })
}

/// The windowed projection: the same value normalization and NaN-gap
/// splitting as [`series_point_runs_for`], but the x coordinates come from the
/// caller's [`WindowSlots`] grid — the identical mapping the chart's hover
/// resolves through, so a capacity-windowed graph and its readout cannot
/// disagree about where a sample sits.
#[must_use]
pub(crate) fn series_point_runs_windowed(
    samples: &[f32],
    size: Size,
    max: f32,
    slots: &WindowSlots,
) -> Vec<Vec<Point>> {
    series_point_runs_with(samples, size, max, &|index| slots.x(index))
}

/// The shared run splitter behind both projections: finite contiguous runs of
/// points, y from [`scaled_y`], x from the injected slot mapping.
fn series_point_runs_with(
    samples: &[f32],
    size: Size,
    max: f32,
    x_of: &dyn Fn(usize) -> f32,
) -> Vec<Vec<Point>> {
    let count = samples.len();
    if count == 0 {
        return Vec::new();
    }
    let height = finite_nonnegative(size.height);
    let mut runs = Vec::new();
    let mut current = Vec::new();
    for (index, &raw) in samples.iter().enumerate() {
        if !raw.is_finite() {
            if !current.is_empty() {
                runs.push(std::mem::take(&mut current));
            }
            continue;
        }
        let x = x_of(index);
        let y = scaled_y(raw, max, height);
        current.push(Point::new(x, y));
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs
}

/// The y-coordinate for one value under the exact normalization
/// [`series_point_runs_for`] applies: `0` at the baseline, `max` at the top
/// edge, out-of-range values clamped inside the frame, and a non-positive or
/// non-finite ceiling (or value) anchoring at the baseline — the honest idle
/// state, never a fabricated mid-line. Pure so a hover dot provably lands on
/// the drawn sample.
#[must_use]
pub(crate) fn scaled_y(value: f32, max: f32, height: f32) -> f32 {
    let height = finite_nonnegative(height);
    let ceiling = if max.is_finite() { max.max(0.0) } else { 0.0 };
    let scale = if ceiling > 0.0 { 1.0 / ceiling } else { 0.0 };
    let value = if value.is_finite() {
        value.clamp(0.0, ceiling)
    } else {
        0.0
    };
    height * (1.0 - value * scale)
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

/// Project normalized `[0,100]` percentage samples into finite contiguous
/// runs — the percentage-specialization of [`series_point_runs_for`].
#[must_use]
pub(crate) fn series_point_runs(samples: &[f32], size: Size) -> Vec<Vec<Point>> {
    series_point_runs_for(samples, size, 100.0)
}

#[cfg(test)]
#[path = "../tests/gui/perf_chart/tests.rs"]
mod tests;
