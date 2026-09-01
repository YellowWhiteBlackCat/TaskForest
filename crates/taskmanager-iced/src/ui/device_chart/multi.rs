//! Two-series per-device graph for split throughput families (a disk's
//! read/write, a NIC's rx/tx), built on the same iced 0.14 `Canvas` +
//! `Program` edge as the single-series [`DeviceChart`] in the parent module.
//!
//! Each series keeps its OWN polyline + Mission-Center area fill, drawn from
//! the shared [`WindowSlots`] grid — the newest sample pins to the right edge
//! and every sample owns a fixed slot of the configured capacity, so a
//! warming window grows right-to-left (GPUI `sample_x` parity) and BOTH
//! series, the hover crosshair, and the readout pill resolve through the ONE
//! mapping, never disagreeing about where a sample sits. Semantic colors stay
//! inside the device family: the primary series wears the family token
//! (theme disk / network chart accent) and the secondary wears the SAME token
//! lifted toward white ([`dual_series_colors`]) — a tint of the family color,
//! never a new product color. A mini legend (color swatch + label per series,
//! top-right) makes the pairing readable without a page-level widget.
//!
//! Honesty rules match the rest of the chart surface: a non-finite sample is
//! an authoritative gap (that series splits its run; the other keeps its own
//! evidence), a series with fewer than two finite samples strokes nothing, and
//! an unavailable value contributes nothing to the hover pill — gaps are never
//! drawn as zero.
//!
//! Cross-frame geometry reuse: the SAME two-cache contract as the parent
//! module (see that header for the rationale). The DATA cache stores the
//! grid + y-axis ticks + both series + legend, cleared only when the data
//! fingerprint changes (either series' generation / shared max / capacity /
//! smooth toggle / legend labels). The OVERLAY cache stores the crosshair +
//! pill, cleared when the hover index OR the data fingerprint changes.
//! Cursor motion therefore never busts the expensive data geometry, and a
//! moved pill never shows a stale reading.

use std::cell::RefCell;
use std::rc::Rc;

use iced::mouse;
use iced::widget::canvas::{self, Cache, Geometry, Path, Stroke};
use iced::widget::{column, text};
use iced::{Color, Point, Rectangle, Size};
use taskmanager_theme::{Theme, tokens};

use super::{GraphPrefs, ReadoutColors, mini_graph_caption};
use crate::app::Message;
use crate::perf_chart::{
    ChartOpts, HoverState, SeriesGeneration, WindowSlots, area_path, draw_grid_opts,
    draw_hover_sample_mark, draw_readout_pill, draw_y_axis_ticks, line_path, scaled_y,
    series_point_runs_windowed, smooth_area_path, smooth_line_path, vertical_area_gradient,
    y_axis_tick_values,
};
use crate::trend_strip::finite_peak;

/// How far the secondary series' color is lifted toward white from the family
/// token — a tint of the SAME color (lighter reads as the paired direction,
/// e.g. solid "read" vs light "write"), never a new product color.
const SECONDARY_TINT_LIFT: f32 = 0.32;
/// The legend swatch size and the legend label font size.
const LEGEND_SWATCH: f32 = 8.0;
const LEGEND_FONT_SIZE: f32 = 10.0;

/// The (primary, secondary) series color pair for one device family: the
/// family token as-is, and the same token lifted toward white by
/// [`SECONDARY_TINT_LIFT`]. Pure so the "no new product color" rule is
/// unit-tested headlessly (the secondary is strictly between the base and
/// white on every channel, alpha untouched).
#[must_use]
pub(crate) fn dual_series_colors(base: Color) -> (Color, Color) {
    let lift = |channel: f32| channel + (1.0 - channel) * SECONDARY_TINT_LIFT;
    (
        base,
        Color::from_rgba(lift(base.r), lift(base.g), lift(base.b), base.a),
    )
}

/// One series of a two-series device graph: its own immutable window, legend
/// label, and stroke color.
#[derive(Clone)]
pub(crate) struct DeviceMultiSeries {
    pub(crate) samples: Rc<[f32]>,
    pub(crate) label: String,
    pub(crate) color: Color,
}

/// Everything the two-series factory needs beyond the theme and caption —
/// bundled to keep the factory under clippy's argument budget (the parent
/// module's `GraphPrefs` pattern). `family_color` is the device family chart
/// token (theme disk / network); the two series colors derive from it through
/// [`dual_series_colors`]. `capacity` is the sample window capacity (the
/// `graph_data_points` setting) that the right-anchored slot grid grows into.
/// `format_value` is the injected unit formatter for the y-axis ticks and the
/// hover pill (the MB/s spelling belongs to the call site, never the chart).
#[derive(Clone)]
pub(crate) struct DeviceMultiGraphSpec {
    pub(crate) primary: DeviceMultiSeries,
    pub(crate) secondary: DeviceMultiSeries,
    pub(crate) family_color: Color,
    pub(crate) capacity: usize,
    pub(crate) format_value: fn(f32) -> String,
    pub(crate) prefs: GraphPrefs,
}

/// The two-series device graph program. Both series share ONE slot grid and
/// ONE shared max (the greater finite peak of the two windows, or the
/// `max_override`), so their amplitudes are directly comparable.
pub(crate) struct DeviceMultiChart {
    pub primary: DeviceMultiSeries,
    pub secondary: DeviceMultiSeries,
    pub max: f32,
    pub capacity: usize,
    pub grid_color: Color,
    pub tick_color: Color,
    pub smooth: bool,
    pub hover: bool,
    pub format_value: fn(f32) -> String,
    pub readout: ReadoutColors,
    pub opts: ChartOpts,
}

impl DeviceMultiChart {
    fn sample_count(&self) -> usize {
        self.primary.samples.len().max(self.secondary.samples.len())
    }

    fn slots(&self, width: f32) -> WindowSlots {
        WindowSlots::new(self.sample_count(), self.capacity, width)
    }

    /// The data fingerprint identifying the grid + ticks + two-series +
    /// legend geometry this program would draw. Pure so the cross-frame
    /// data-cache-clear gate is unit-tested without a renderer.
    #[must_use]
    pub(crate) fn fingerprint(&self) -> DeviceMultiDataFingerprint {
        DeviceMultiDataFingerprint {
            primary: SeriesGeneration::new(&self.primary.samples),
            secondary: SeriesGeneration::new(&self.secondary.samples),
            max_bits: self.max.to_bits(),
            capacity: self.capacity,
            smooth: self.smooth,
            primary_label: self.primary.label.clone(),
            secondary_label: self.secondary.label.clone(),
        }
    }
}

/// The cached identity of the two-series graph's DATA geometry: both series'
/// immutable snapshot generations, the shared max, the window capacity, the
/// smoothing policy, and the legend labels (the legend text is drawn into the
/// data layer, so a relabel must rebuild it). Colors/theme tokens are NOT
/// part of the fingerprint (a theme switch is rare and one stale-color frame
/// is acceptable — the established two-cache contract).
#[derive(Clone, Default, PartialEq, Debug)]
pub(crate) struct DeviceMultiDataFingerprint {
    primary: SeriesGeneration,
    secondary: SeriesGeneration,
    max_bits: u32,
    capacity: usize,
    smooth: bool,
    primary_label: String,
    secondary_label: String,
}

/// The cached identity of the OVERLAY geometry (crosshair + pill): the
/// hovered sample index plus the data fingerprint — the pill text is re-read
/// from both series at draw time, so the overlay must rebuild when EITHER the
/// hover moves OR the data ticks.
#[derive(Clone, Default, PartialEq, Debug)]
struct DeviceMultiOverlayFingerprint {
    hover_index: Option<usize>,
    data: DeviceMultiDataFingerprint,
}

/// The persistent per-canvas state: the [`HoverState`] plus the TWO physically
/// separate caches and their fingerprints (the parent module's contract).
/// `Default`-derivable so iced can seed one when the canvas node first
/// appears.
#[derive(Default)]
pub(crate) struct DeviceMultiChartState {
    pub(crate) hover: HoverState,
    data_cache: Cache,
    overlay_cache: Cache,
    data_fingerprint: RefCell<DeviceMultiDataFingerprint>,
    overlay_fingerprint: RefCell<DeviceMultiOverlayFingerprint>,
}

impl canvas::Program<Message> for DeviceMultiChart {
    type State = DeviceMultiChartState;

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
                let slots = self.slots(bounds.width);
                let next = cursor
                    .position_over(bounds)
                    .and_then(|position| slots.index_at(position.x));
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
        // The parent module's two-cache contract:
        //  - DATA cache: grid + ticks + both series + legend. Cleared only on
        //    a data-fingerprint change. Cursor motion NEVER busts this.
        //  - OVERLAY cache: crosshair + pill. Cleared when the hover index OR
        //    the data fingerprint changes.
        let data_fp = self.fingerprint();
        if *state.data_fingerprint.borrow() != data_fp {
            *state.data_fingerprint.borrow_mut() = data_fp.clone();
            state.data_cache.clear();
        }
        let overlay_fp = DeviceMultiOverlayFingerprint {
            hover_index: state.hover.index,
            data: data_fp,
        };
        if *state.overlay_fingerprint.borrow() != overlay_fp {
            *state.overlay_fingerprint.borrow_mut() = overlay_fp;
            state.overlay_cache.clear();
        }

        let primary = &self.primary;
        let secondary = &self.secondary;
        let max = self.max;
        let grid_color = self.grid_color;
        let tick_color = self.tick_color;
        let smooth = self.smooth;
        let opts = self.opts;
        let format_value = self.format_value;
        let capacity = self.capacity;
        let sample_count = self.sample_count();
        let data_geometry = state.data_cache.draw(renderer, bounds.size(), |frame| {
            let size = frame.size();
            draw_grid_opts(frame, size, grid_color, opts);
            if super::axis_ticks_visible(size.height) {
                let ticks = y_axis_tick_values(max);
                draw_y_axis_ticks(frame, size, &ticks, max, format_value, tick_color);
            }
            // Secondary first (drawn under primary) so the family-solid
            // series sits on top at any crossing. Each series splits its own
            // NaN gaps and keeps its own slot positions.
            let slots = WindowSlots::new(sample_count, capacity, size.width);
            draw_multi_series(frame, size, &slots, secondary, max, smooth, opts);
            draw_multi_series(frame, size, &slots, primary, max, smooth, opts);
            draw_chart_legend(
                frame,
                size,
                &[
                    (primary.label.as_str(), primary.color),
                    (secondary.label.as_str(), secondary.color),
                ],
                tick_color,
            );
        });

        let hover_enabled = self.hover;
        let hover_index = state.hover.index;
        let readout = self.readout;
        let overlay_geometry = state.overlay_cache.draw(renderer, bounds.size(), |frame| {
            if !hover_enabled {
                return;
            }
            let Some(index) = hover_index else {
                return;
            };
            let size = frame.size();
            let slots = WindowSlots::new(sample_count, capacity, size.width);
            let x = slots.x(index);
            // GPUI-parity crosshair: the vertical rule comes from the shared
            // pill; each series that holds a finite value at the index gets a
            // horizontal rule + snap dot on its own sample (the primary's
            // rule; the secondary shares the pill's x). A gap in one series
            // never fabricates a mark.
            if let Some(&value) = primary.samples.get(index).filter(|value| value.is_finite()) {
                draw_hover_sample_mark(
                    frame,
                    size,
                    x,
                    scaled_y(value, max, size.height),
                    primary.color,
                    grid_color,
                );
            }
            if let Some(&value) = secondary
                .samples
                .get(index)
                .filter(|value| value.is_finite())
            {
                draw_hover_sample_mark(
                    frame,
                    size,
                    x,
                    scaled_y(value, max, size.height),
                    secondary.color,
                    grid_color,
                );
            }
            draw_readout_pill(
                frame,
                size,
                x,
                multi_readout_text(primary, secondary, index, format_value).as_deref(),
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

/// Stroke + fill one series of the two-series graph through the shared slot
/// grid (the parent module's single-series stroke/fill pair, fed by
/// [`series_point_runs_windowed`]). Fewer than two points in a run strokes
/// nothing — the honest collecting/gap state.
fn draw_multi_series(
    frame: &mut canvas::Frame<iced::Renderer>,
    size: Size,
    slots: &WindowSlots,
    series: &DeviceMultiSeries,
    max: f32,
    smooth: bool,
    opts: ChartOpts,
) {
    for points in series_point_runs_windowed(&series.samples, size, max, slots) {
        if points.len() < 2 {
            continue;
        }
        if smooth && points.len() >= 3 {
            if let Some(area) = smooth_area_path(&points, size.height) {
                frame.fill(&area, vertical_area_gradient(series.color, size.height));
            }
            if let Some(line) = smooth_line_path(&points) {
                frame.stroke(
                    &line,
                    Stroke::default()
                        .with_width(opts.stroke_width)
                        .with_color(series.color),
                );
            }
        } else {
            if let Some(area) = area_path(&points, size.height) {
                frame.fill(&area, vertical_area_gradient(series.color, size.height));
            }
            if let Some(line) = line_path(&points) {
                frame.stroke(
                    &line,
                    Stroke::default()
                        .with_width(opts.stroke_width)
                        .with_color(series.color),
                );
            }
        }
    }
}

/// Draw the mini legend top-right: one color swatch + label per series, in
/// the caller's order (primary first). Text width is the pill's fixed
/// per-glyph estimate (the canvas has no text measurement without a
/// renderer) — a layout approximation, not a measured glyph width.
fn draw_chart_legend(
    frame: &mut canvas::Frame<iced::Renderer>,
    size: Size,
    entries: &[(&str, Color)],
    text_color: Color,
) {
    let entries: Vec<(&str, Color)> = entries
        .iter()
        .copied()
        .filter(|(label, _)| !label.is_empty())
        .collect();
    if entries.is_empty() {
        return;
    }
    let swatch_gap = 4.0;
    let entry_gap = 12.0;
    let text_width = |label: &str| label.chars().count() as f32 * LEGEND_FONT_SIZE * 0.6;
    let total: f32 = entries
        .iter()
        .map(|(label, _)| LEGEND_SWATCH + swatch_gap + text_width(label) + entry_gap)
        .sum();
    let mut x = (size.width - total - 6.0).max(6.0);
    for (label, color) in entries {
        frame.fill(
            &Path::rectangle(Point::new(x, 5.0), Size::new(LEGEND_SWATCH, LEGEND_SWATCH)),
            color,
        );
        frame.fill_text(canvas::Text {
            content: label.to_string(),
            position: Point::new(x + LEGEND_SWATCH + swatch_gap, 5.0 + LEGEND_SWATCH / 2.0),
            color: text_color,
            size: iced::Pixels(LEGEND_FONT_SIZE),
            align_y: iced::alignment::Vertical::Center,
            ..canvas::Text::default()
        });
        x += LEGEND_SWATCH + swatch_gap + text_width(label) + entry_gap;
    }
}

/// The hover pill's text at one sample index across BOTH series: each series
/// that actually holds a finite value at the index contributes
/// `"<label> <formatted value>"`, joined by `" · "` — mirroring the CPU
/// chart's readout composition. `None` when neither series has evidence there
/// (a shared gap) — the pill is suppressed, the crosshair line stays. Pure so
/// the composition is unit-tested headlessly.
#[must_use]
pub(crate) fn multi_readout_text(
    primary: &DeviceMultiSeries,
    secondary: &DeviceMultiSeries,
    index: usize,
    format_value: fn(f32) -> String,
) -> Option<String> {
    let mut parts = Vec::new();
    for series in [primary, secondary] {
        if let Some(&value) = series.samples.get(index).filter(|value| value.is_finite()) {
            parts.push(format!("{} {}", series.label, format_value(value)));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

/// Build the primary two-series device graph used by a Performance device
/// page, under the shared compact/wide height contract
/// ([`super::primary_graph_height`], the single-series
/// `device_mini_graph_fill` rule): a wide detail card gives the canvas the
/// remaining column height (`Length::Fill`), while a compact card lives
/// inside an unbounded `Scrollable` and keeps the readable fixed primary
/// height, letting the outer page own overflow.
pub(crate) fn device_multi_graph_fill(
    spec: DeviceMultiGraphSpec,
    caption: String,
    theme_snapshot: &Theme,
    compact: bool,
) -> iced::Element<'static, Message, iced::Theme, iced::Renderer> {
    device_multi_graph_with_length(
        spec,
        caption,
        theme_snapshot,
        super::primary_graph_height(compact),
    )
}

fn device_multi_graph_with_length(
    spec: DeviceMultiGraphSpec,
    caption: String,
    theme_snapshot: &Theme,
    height: iced::Length,
) -> iced::Element<'static, Message, iced::Theme, iced::Renderer> {
    let DeviceMultiGraphSpec {
        mut primary,
        mut secondary,
        family_color,
        capacity,
        format_value,
        prefs,
    } = spec;
    let (primary_color, secondary_color) = dual_series_colors(family_color);
    primary.color = primary_color;
    secondary.color = secondary_color;
    let max = prefs
        .max_override
        .unwrap_or_else(|| finite_peak(&primary.samples).max(finite_peak(&secondary.samples)));
    let caption_color = crate::theme_binding::color(theme_snapshot.palette().fg_muted);
    let grid_color = crate::theme_binding::color(theme_snapshot.palette().border);
    let label = mini_graph_caption(&caption, primary.samples.len().max(secondary.samples.len()));
    let chart = DeviceMultiChart {
        primary,
        secondary,
        max,
        capacity,
        grid_color,
        tick_color: caption_color,
        smooth: prefs.smooth,
        hover: prefs.hover,
        format_value,
        readout: ReadoutColors {
            bg: crate::theme_binding::color(theme_snapshot.palette().surface),
            fg: crate::theme_binding::color(theme_snapshot.palette().fg),
        },
        opts: ChartOpts::default(),
    };
    column(vec![
        text(label)
            .size(f32::from(tokens::FONT_12))
            .color(caption_color)
            .into(),
        canvas::Canvas::new(chart)
            .width(iced::Length::Fill)
            .height(height)
            .into(),
    ])
    .spacing(4)
    .into()
}
