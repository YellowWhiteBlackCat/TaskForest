//! Per-device mini-graph for the Performance-page device sections (Disk /
//! Network / GPU / Battery / Fan), built on the same iced 0.14 `Canvas` +
//! `Program` edge as [`crate::perf_chart`].
//!
//! Each device row carries its OWN single-series trend — read+write bytes/sec
//! for a disk, rx+tx bytes/sec for a NIC, utilization percent for a GPU —
//! plotted as a polyline plus a Mission-Center-style area fill (the shared
//! vertical gradient from [`crate::perf_chart::vertical_area_gradient`]), the
//! same treatment the Cpu view gives its CPU/memory chart. The point geometry
//! reuses the shared [`crate::perf_chart::series_point_runs_for`] projection
//! (already unit-tested) so a per-device mini-graph and the full chart never
//! disagree on how a sample maps onto a frame; this module only owns the
//! one-series stroke + fill. Read-only — the graph produces no messages.
//!
//! Honesty matches the rest of the typed surface: each graph plots the device's
//! OWN per-device window from [`LiveGraphHistory`] (`disk_bytes_per_sec_for` /
//! `network_bytes_per_sec_for` / `gpu_usage_pct_for` / `battery_capacity_pct_for`
//! / `fan_rpm_for`) — never a fabricated flat line. Magnitude series (bytes/sec,
//! RPM) auto-scale to their finite peak so traffic moves the line instead of
//! clamping flat against the 100% ceiling; GPU% and battery charge % are fixed
//! 0..100. A window with fewer than two samples strokes nothing and the caption
//! carries the localized collecting suffix — the honest collecting state.
//!
//! Cross-frame geometry reuse: iced repaints at vsync (~60 Hz) while a device's
//! window advances only ~1 Hz and the cursor moves independently. The state
//! therefore holds TWO physically separate [`canvas::Cache`]s (the round-1
//! `process_sparkline` pattern, extended to a hover chart): the DATA cache
//! stores the grid + single series (cleared only when the data fingerprint
//! changes — new sample / window shift / auto-scale max / smooth toggle), and
//! the OVERLAY cache stores the hover readout pill (cleared when the hover
//! index OR the data fingerprint changes; a no-op when hover is disabled).
//! Cursor motion therefore never busts the expensive data geometry, and a moved
//! pill never shows a stale reading.

use std::cell::RefCell;
use std::rc::Rc;

use iced::mouse;
use iced::widget::canvas::{self, Cache, Geometry};
use iced::widget::{column, text};
use iced::{Color, Length, Rectangle};
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::graph_summary;
use taskmanager_shell::presentation::trend::TrendSeries;
use taskmanager_theme::{Theme, tokens};

use super::quantity_text_pref;
use crate::app::Message;
use crate::perf_chart::{
    ChartOpts, HoverState, ReadoutColors, SeriesGeneration, WindowSlots, area_path, draw_grid_opts,
    draw_hover_sample_mark, draw_readout_pill, draw_y_axis_ticks, hovered_index, line_path,
    scaled_y, series_point_runs_for, y_axis_tick_values,
};
use crate::trend_strip::finite_peak;

/// Primary per-device canvas height. GPUI gives the selected device's main
/// graph the dominant vertical region; Iced therefore must not compress it to
/// a sidebar-sized sparkline.
pub(crate) const DEVICE_CHART_HEIGHT: f32 = 260.0;
/// Compact height for secondary engine/power/temperature graphs.
pub(crate) const SECONDARY_DEVICE_CHART_HEIGHT: f32 = 128.0;
/// Engine graphs are compact supporting evidence beneath the GPU aggregate;
/// keeping them short prevents a multi-engine card from clipping its last row.
pub(crate) const ENGINE_DEVICE_CHART_HEIGHT: f32 = 56.0;
/// Grid/stroke knobs the single-series graph draws with. The default IS the
/// legacy look (quarter grid, six vertical rules, border token at 0.48, 1.6
/// stroke) — the parameterization exists so the two-series graph and any
/// future variant share the same authority.
const CHART_OPTS: ChartOpts = ChartOpts::DEFAULT;
/// The percentage ceiling shared by every percentage-typed series.
const PERCENT_MAX: f32 = 100.0;
/// Minimum frame height that carries y-axis tick labels: the 260px primary
/// and 128px secondary graphs read them; the 56px engine strips stay clean.
const AXIS_TICK_MIN_HEIGHT: f32 = 96.0;
/// Alpha applied to the foreground token for tick labels (the pill's fg token
/// dimmed to a quiet caption weight — token-derived, never a literal).
const AXIS_TICK_ALPHA: f32 = 0.55;

/// Whether a graph frame is tall enough to carry y-axis tick labels. Pure so
/// the height policy is unit-tested headlessly.
#[must_use]
pub(crate) fn axis_ticks_visible(height: f32) -> bool {
    height.is_finite() && height >= AXIS_TICK_MIN_HEIGHT
}

/// One per-device series plotted as a polyline plus an area fill. Built fresh
/// each render from that device's window and a resolved (token-derived) stroke
/// color; the `max` field maps the top of the frame (`100.0` for percentages,
/// the finite peak for bytes/sec). `smooth` routes the stroke + fill top edge
/// through the Catmull-Rom spline (GPUI smooth-graphs parity). When `hover` is
/// set the canvas is hover-interactive at ANY of the three chart heights (the
/// flag is owned by the page factory — the 260px main graphs, and the 128px
/// secondary / 56px engine graphs wherever the page opts in): cursor motion
/// updates the persistent [`HoverState`] and the draw pass renders a
/// GPUI-parity crosshair — vertical rule + value pill at the hovered sample's
/// slot x, horizontal rule and snap dot at the sample's y — formatted by the
/// graph's own [`DeviceMetricScale`] (`scale`) in the same unit family as the
/// caption. The crosshair, the pill, and the drawn polyline all resolve
/// through the ONE shared [`WindowSlots`] mapping, so the readout never
/// disagrees with a visible sample.
pub(crate) struct DeviceChart {
    pub samples: Rc<[f32]>,
    pub color: Color,
    pub max: f32,
    pub grid_color: Color,
    pub smooth: bool,
    /// Hover readout enabled (the main per-device graphs).
    pub hover: bool,
    /// The unit family the hover readout formats its value in — the same
    /// [`DeviceMetricScale`] that picked the Y ceiling.
    pub scale: DeviceMetricScale,
    /// Token-derived pill colors for the hover readout.
    pub readout: ReadoutColors,
}

impl DeviceChart {
    /// The data fingerprint identifying the grid + single-series geometry this
    /// program would draw for its current data. Pure so the cross-frame
    /// data-cache-clear gate is unit-tested without a renderer (see
    /// [`DeviceChartDataFingerprint`]).
    #[must_use]
    pub(crate) fn fingerprint(&self) -> DeviceChartDataFingerprint {
        DeviceChartDataFingerprint::from_window(&self.samples, self.max, self.smooth)
    }
}

/// The cached identity of the device graph's DATA geometry (grid + single
/// series): immutable snapshot generation, auto-scale `max`, and smoothing.
/// The `max` field is critical for magnitude
/// series (bytes/sec, RPM) — without it, a traffic-level change that moved the
/// finite peak would reuse stale geometry clamped to the old ceiling. The
/// `smooth` flag is in the fingerprint so toggling smoothing rebuilds (otherwise
/// the cached polyline path would be the wrong family). Colors/theme tokens are
/// NOT part of the fingerprint (a theme switch is rare and one stale-color frame
/// is acceptable — matches round-1 `process_sparkline`).
#[derive(Clone, Default, PartialEq, Debug)]
pub(crate) struct DeviceChartDataFingerprint {
    samples: SeriesGeneration,
    max_bits: u32,
    smooth: bool,
}

impl DeviceChartDataFingerprint {
    /// Build the data fingerprint from generation, scale, and smoothing.
    #[must_use]
    fn from_window(samples: &Rc<[f32]>, max: f32, smooth: bool) -> Self {
        Self {
            samples: SeriesGeneration::new(samples),
            max_bits: max.to_bits(),
            smooth,
        }
    }
}

/// The cached identity of the device graph's OVERLAY geometry (hover readout
/// pill): the hovered sample index plus the data fingerprint. The pill text is
/// read from the samples at draw time, so the overlay must rebuild when EITHER
/// the hover moves OR the underlying data ticks (otherwise a moved cursor would
/// show a stale reading). When hover is disabled the overlay is always empty
/// and the fingerprint stays constant, so the overlay cache never busts.
#[derive(Clone, Default, PartialEq, Debug)]
struct DeviceChartOverlayFingerprint {
    hover_index: Option<usize>,
    data: DeviceChartDataFingerprint,
}

/// The persistent per-canvas state for a device mini-graph: the [`HoverState`]
/// (updated through [`canvas::Program::update`] on cursor motion, when hover is
/// enabled), plus TWO physically separate [`canvas::Cache`]s — the data cache
/// (grid + series, cleared only on a data-fingerprint change) and the overlay
/// cache (hover readout pill, cleared on a hover-index OR data-fingerprint
/// change). `Default`-derivable so iced can seed one when the canvas node first
/// appears.
#[derive(Default)]
pub(crate) struct DeviceChartState {
    /// The hovered sample index, updated through `update` on cursor motion
    /// (when hover is enabled).
    pub(crate) hover: HoverState,
    data_cache: Cache,
    overlay_cache: Cache,
    data_fingerprint: RefCell<DeviceChartDataFingerprint>,
    overlay_fingerprint: RefCell<DeviceChartOverlayFingerprint>,
}

impl canvas::Program<Message> for DeviceChart {
    type State = DeviceChartState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        if !self.hover {
            return None;
        }
        match event {
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let next = cursor.position_over(bounds).and_then(|position| {
                    hovered_index(position.x, bounds.width, self.samples.len())
                });
                if next != state.hover.index {
                    state.hover.index = next;
                    return Some(canvas::Action::request_redraw());
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorLeft) if state.hover.index.is_some() => {
                state.hover.index = None;
                return Some(canvas::Action::request_redraw());
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        // TWO physically separate caches (the round-1 process_sparkline pattern,
        // extended to a hover chart):
        //  - DATA cache: grid + single series. Cleared only when the data
        //    fingerprint changes (new sample / window shift / max change /
        //    smooth toggle). Cursor motion NEVER busts this.
        //  - OVERLAY cache: hover readout pill. Cleared when the hover index OR
        //    the data fingerprint changes (when hover is enabled). When hover
        //    is disabled the overlay is a constant empty frame and the cache
        //    never busts.
        let data_fp = self.fingerprint();
        if *state.data_fingerprint.borrow() != data_fp {
            *state.data_fingerprint.borrow_mut() = data_fp.clone();
            state.data_cache.clear();
        }
        let overlay_fp = DeviceChartOverlayFingerprint {
            hover_index: state.hover.index,
            data: data_fp,
        };
        if *state.overlay_fingerprint.borrow() != overlay_fp {
            *state.overlay_fingerprint.borrow_mut() = overlay_fp;
            state.overlay_cache.clear();
        }

        // The cache closure is synchronous, so a cache hit does not need a
        // fresh copy of the device history window.
        let samples = &self.samples;
        let color = self.color;
        let max = self.max;
        let grid_color = self.grid_color;
        let smooth = self.smooth;
        let scale = self.scale;
        let tick_color = self.readout.fg.scale_alpha(AXIS_TICK_ALPHA);
        let data_geometry = state.data_cache.draw(renderer, bounds.size(), |frame| {
            let size = frame.size();
            draw_grid_opts(frame, size, grid_color, CHART_OPTS);
            // Y-axis tick labels in the graph's own unit family (the same
            // summary rule the caption and hover pill use), only in graphs
            // tall enough to read them — the 260px primary and 128px
            // secondary graphs; the 56px engine strips stay clean.
            if axis_ticks_visible(size.height) {
                let ticks = y_axis_tick_values(max);
                draw_y_axis_ticks(
                    frame,
                    size,
                    &ticks,
                    max,
                    |value| summary_value(scale, value),
                    tick_color,
                );
            }
            for points in series_point_runs_for(samples, size, max) {
                if points.len() < 2 {
                    continue;
                }
                if smooth && points.len() >= 3 {
                    if let Some(area) = crate::perf_chart::smooth_area_path(&points, size.height) {
                        frame.fill(
                            &area,
                            crate::perf_chart::vertical_area_gradient(color, size.height),
                        );
                    }
                    if let Some(line) = crate::perf_chart::smooth_line_path(&points) {
                        frame.stroke(
                            &line,
                            canvas::Stroke::default()
                                .with_width(CHART_OPTS.stroke_width)
                                .with_color(color),
                        );
                    }
                } else {
                    if let Some(area) = area_path(&points, size.height) {
                        frame.fill(
                            &area,
                            crate::perf_chart::vertical_area_gradient(color, size.height),
                        );
                    }
                    if let Some(line) = line_path(&points) {
                        frame.stroke(
                            &line,
                            canvas::Stroke::default()
                                .with_width(CHART_OPTS.stroke_width)
                                .with_color(color),
                        );
                    }
                }
            }
        });

        let readout = self.readout;
        let hover_enabled = self.hover;
        let hover_index = state.hover.index;
        let overlay_geometry = state.overlay_cache.draw(renderer, bounds.size(), |frame| {
            if !hover_enabled {
                return;
            }
            let Some(index) = hover_index else {
                return;
            };
            let size = frame.size();
            // The crosshair and the pill share ONE slot mapping
            // (`WindowSlots::spread`) with the drawn series — the vertical
            // rule, the snap dot, and the readout all land on the sample the
            // cursor is over, never beside it (GPUI hover parity).
            let slots = WindowSlots::spread(samples.len(), size.width);
            let x = slots.x(index);
            if let Some(&value) = samples.get(index).filter(|value| value.is_finite()) {
                draw_hover_sample_mark(
                    frame,
                    size,
                    x,
                    scaled_y(value, max, size.height),
                    color,
                    grid_color,
                );
            }
            draw_readout_pill(
                frame,
                size,
                x,
                device_readout_text(scale, samples, index).as_deref(),
                grid_color,
                readout,
            );
        });

        vec![data_geometry, overlay_geometry]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.hover && cursor.position_over(bounds).is_some() {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

/// Two-series (split-direction) device graphs: the disk read/write and NIC
/// rx/tx form of this module's single-series graph, sharing the WindowSlots
/// mapping, the y-axis tick ladder, and the two-cache contract. Consumed by
/// the Disk/Network Performance pages.
pub(crate) mod multi;
pub(crate) mod scale;
pub(crate) use scale::{
    DeviceMetricScale, device_readout_text, mini_graph_summary, series_max, summary_value,
};

/// The per-device mini-graph caption: the metric label alone once a window has
/// at least two samples to plot, else the label suffixed with the localized
/// collecting phrase so an empty or just-launched per-device window reads as
/// collecting rather than a blank graph. Pure so the honesty rule is
/// unit-tested without a renderer.
#[must_use]
pub(crate) fn mini_graph_caption(metric_label: &str, sample_count: usize) -> String {
    if sample_count >= 2 {
        metric_label.to_string()
    } else {
        format!("{metric_label} · {}", t("graph.collecting"))
    }
}

/// Build one device's mini-graph: a small muted caption (the metric label,
/// suffixed "· collecting" while the window holds fewer than two samples) above
/// one single-series auto-scaled `Canvas`. `samples` is the device's OWN
/// per-device window (already resolved by the caller from `LiveGraphHistory`
/// `::{disk_bytes_per_sec_for, network_bytes_per_sec_for, gpu_usage_pct_for,
/// battery_capacity_pct_for, fan_rpm_for}`); `scale` picks the scaling rule
/// ([`series_max`]). The canvas strokes nothing until the window holds at least
/// two finite samples (its caption stays) — the honest collecting state, never a
/// fabricated flat line. The returned element owns its samples/color/caption
/// (Copy color + shared `Rc`/`String`), so it is `'static` and stacks cleanly
/// inside a device block.
pub(crate) fn device_mini_graph<S: Into<Rc<[f32]>>>(
    samples: S,
    scale: impl Into<DeviceMetricScale>,
    color: Color,
    caption: String,
    theme_snapshot: &Theme,
    prefs: GraphPrefs,
) -> iced::Element<'static, Message, iced::Theme, iced::Renderer> {
    device_mini_graph_with_height(
        samples,
        scale,
        color,
        caption,
        theme_snapshot,
        DEVICE_CHART_HEIGHT,
        prefs,
    )
}

/// Build the primary graph used by a generic Performance device page. Wide
/// detail cards give this graph the remaining column height; compact cards
/// live inside an unbounded `Scrollable`, so they keep the readable fixed
/// height and let the outer page own overflow.
pub(crate) fn device_mini_graph_fill<S: Into<Rc<[f32]>>>(
    samples: S,
    scale: impl Into<DeviceMetricScale>,
    color: Color,
    caption: String,
    theme_snapshot: &Theme,
    compact: bool,
    prefs: GraphPrefs,
) -> iced::Element<'static, Message, iced::Theme, iced::Renderer> {
    device_mini_graph_with_length(
        samples,
        scale,
        color,
        caption,
        theme_snapshot,
        primary_graph_height(compact),
        prefs,
    )
}

/// The primary graph's height policy is the shared compact/wide contract for
/// Disk, Network, GPU, Battery and Fan. Keeping the decision here prevents
/// each page from reintroducing a separate hard-coded wide height.
#[must_use]
pub(crate) fn primary_graph_height(compact: bool) -> Length {
    if compact {
        Length::Fixed(DEVICE_CHART_HEIGHT)
    } else {
        Length::Fill
    }
}

/// One mini-graph's frontend-local presentation preferences: the smoothed-
/// spline flag, the explicit Y ceiling override (network dynamic
/// scaling), and the hover-readout switch (the 260px main per-device graphs,
/// and any 128px secondary / 56px engine graph whose page opts in — the
/// crosshair works at every height). Bundled so the graph factories stay
/// under clippy's argument budget.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GraphPrefs {
    pub(crate) smooth: bool,
    pub(crate) max_override: Option<f32>,
    /// Hover readout (reference line + value pill) for the main per-device
    /// graphs; secondary engine/power/temperature graphs keep it off.
    pub(crate) hover: bool,
}

/// Build one graph with an explicit height for a secondary series. The scale,
/// caption and honest collecting behavior stay identical to the primary graph;
/// only the layout contract changes.
pub(crate) fn device_mini_graph_with_height<S: Into<Rc<[f32]>>>(
    samples: S,
    scale: impl Into<DeviceMetricScale>,
    color: Color,
    caption: String,
    theme_snapshot: &Theme,
    height: f32,
    prefs: GraphPrefs,
) -> iced::Element<'static, Message, iced::Theme, iced::Renderer> {
    device_mini_graph_with_length(
        samples,
        scale,
        color,
        caption,
        theme_snapshot,
        Length::Fixed(height),
        prefs,
    )
}

fn device_mini_graph_with_length<S: Into<Rc<[f32]>>>(
    samples: S,
    scale: impl Into<DeviceMetricScale>,
    color: Color,
    caption: String,
    theme_snapshot: &Theme,
    height: Length,
    prefs: GraphPrefs,
) -> iced::Element<'static, Message, iced::Theme, iced::Renderer> {
    let scale = scale.into();
    let samples = samples.into();
    let max = prefs
        .max_override
        .unwrap_or_else(|| series_max(scale, &samples));
    let caption_color = taskmanager_theme::iced::color(theme_snapshot.palette().fg_muted);
    let grid_color = taskmanager_theme::iced::color(theme_snapshot.palette().border);
    // The DeviceChart draws nothing for fewer than two points; surface that in
    // the caption so an empty/just-launched window reads as collecting, not as a
    // blank graph. History windows are finite-only, so len == finite count.
    let label = mini_graph_caption(&caption, samples.len());
    let label = mini_graph_summary(scale, &samples)
        .map(|summary| format!("{label} · {summary}"))
        .unwrap_or(label);
    // The hover pill wears the same elevated-surface/foreground pair as the CPU
    // chart's readout (token-derived, never a literal).
    let readout = ReadoutColors {
        bg: taskmanager_theme::iced::color(theme_snapshot.palette().surface),
        fg: taskmanager_theme::iced::color(theme_snapshot.palette().fg),
    };
    column(vec![
        text(label)
            .size(f32::from(tokens::FONT_12))
            .color(caption_color)
            .into(),
        canvas::Canvas::new(DeviceChart {
            samples,
            color,
            max,
            grid_color,
            smooth: prefs.smooth,
            hover: prefs.hover,
            scale,
            readout,
        })
        .width(Length::Fill)
        .height(height)
        .into(),
    ])
    .spacing(4)
    .into()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/device_chart/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/gui/device_chart_tests.rs"]
mod device_chart_tests;
