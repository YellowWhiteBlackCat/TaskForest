//! Per-core utilization grid for the Performance page's Cpu view.
//!
//! One mini cell per logical core, projected from the SHARED `LiveGraphHistory`
//! per-core windows (the same single source the TUI grid and the gpui mini-
//! graphs read). Each cell renders a rolling mini-HISTORY sparkline of its
//! whole window — a tiny line+area `Canvas` (reusing the Performance chart's
//! `[0,100]` point projection) — with the LATEST sample tinting the line by
//! load tier: green up to 60%, amber 60–85%, red above. A pinned core therefore
//! pops at a glance AND shows its recent trend, matching the gpui per-core cell
//! (which renders a rolling mini-history per logical core). The "XX%" readout
//! stays beneath the sparkline. A core with no finite sample yet renders an
//! honest dash, never a fabricated 0%; a core with a single sample draws no
//! line (the readout carries the value) until a second snapshot arrives.
//!
//! Cross-frame geometry reuse: iced repaints at vsync (~60 Hz) while the
//! per-core windows advance only ~1 Hz, so most frames would recompute
//! identical line+area geometry. The [`canvas::Program::State`] therefore holds
//! a [`canvas::Cache`] plus a [`CoreCellFingerprint`]; the cache returns the
//! stored [`Geometry`] unchanged when the canvas bounds are stable AND the cache
//! has not been cleared, and [`CoreCellChart::draw`] clears it only when the
//! fingerprint changed (new sample / window shift) — the same pattern
//! `process_sparkline` verified. The tier color derives from the latest sample
//! value, so the immutable snapshot generation also captures a tier change; the
//! theme token is NOT in the fingerprint (a theme switch is rare and one
//! stale-color frame is acceptable, matching round-1 `process_sparkline`).

use std::cell::RefCell;
use std::rc::Rc;

use iced::mouse;
use iced::widget::canvas::{self, Cache, Geometry};
use iced::widget::{column, container, row, text};
use iced::{Color, Length, Rectangle};
use taskmanager_application::i18n::t;
use taskmanager_shell::presentation::MISSING_VALUE;
use taskmanager_theme::{Theme, tokens};

use crate::app::Message;
use crate::perf_chart::{SeriesGeneration, area_path, line_path, series_point_runs};
use crate::theme;

/// Cells per row in the grid.
const CELLS_PER_ROW: usize = 6;
/// Load-tier band edges (percent): below [`WARN_EDGE`] is green, up to
/// [`DANGER_EDGE`] is amber, above is red.
const WARN_EDGE: f32 = 60.0;
const DANGER_EDGE: f32 = 85.0;
/// Fixed per-cell sparkline height (a touch taller than the app-history row
/// sparkline so a pinned core's recent history reads at a glance inside the
/// cell). Width is `Fill` against the cell so the line tracks the column.
const CELL_CHART_HEIGHT: f32 = 24.0;
/// Stroke width for the per-core sparkline polyline (mirrors the app-history
/// sparkline — one series needs less weight than the two-series Cpu chart).
const CELL_STROKE_WIDTH: f32 = 1.4;
/// Alpha multiplier applied to the tier color for the area fill (matches the
/// Cpu chart's legibility wash).
const CELL_AREA_ALPHA: f32 = 0.18;

type Elem<'a> = iced::Element<'a, Message, iced::Theme, iced::Renderer>;

/// The Cpu-only per-core utilization grid. A core with no finite sample yet
/// renders an honest dash — never a fabricated 0%.
pub(crate) fn per_core_grid_panel<'a>(app: &crate::IcedApp, theme_snapshot: &'a Theme) -> Elem<'a> {
    let series = app.cached_per_core_usage_series();
    if series.is_empty() {
        return container(text(t("common.collecting_telemetry")).size(f32::from(tokens::FONT_13)))
            .style(move |_| theme::panel_style(theme_snapshot))
            .into();
    }

    let hw = app.shell.projection().hardware.as_ref();
    let breakdown_label = hw.and_then(|h| {
        if h.core_breakdown.total() > 0 {
            let mut parts = Vec::new();
            if h.core_breakdown.p_cores > 0 {
                parts.push(format!("{} P-Cores", h.core_breakdown.p_cores));
            }
            if h.core_breakdown.e_cores > 0 {
                parts.push(format!("{} E-Cores", h.core_breakdown.e_cores));
            }
            if h.core_breakdown.lp_cores > 0 {
                parts.push(format!("{} LP-Cores", h.core_breakdown.lp_cores));
            }
            Some(parts.join(" · "))
        } else {
            None
        }
    });

    let cpu_types = hw.map(|h| &h.cpu_types);

    // The app cache owns one contiguous snapshot per core for this history
    // revision. Group the shared handles directly so idle frames allocate no
    // fresh VecDeque copies (and no per-cell sample buffers).
    let mut grouped: Vec<Vec<(usize, Rc<[f32]>)>> = Vec::new();
    for (index, samples) in series.iter().cloned().enumerate() {
        if grouped
            .last()
            .is_none_or(|current| current.len() == CELLS_PER_ROW)
        {
            grouped.push(Vec::with_capacity(CELLS_PER_ROW));
        }
        if let Some(current) = grouped.last_mut() {
            current.push((index, samples));
        }
    }
    let rows: Vec<Elem<'static>> = grouped
        .into_iter()
        .map(|chunk| {
            let cells: Vec<Elem<'static>> = chunk
                .into_iter()
                .map(|(index, samples)| {
                    let core_type = cpu_types.and_then(|types| types.get(index)).copied();
                    core_grid_cell(index, samples, theme_snapshot, core_type)
                })
                .collect();
            row(cells).spacing(8).width(Length::Fill).into()
        })
        .collect();

    let mut header_items: Vec<Elem<'a>> = vec![
        text(t("common.cores"))
            .size(f32::from(tokens::FONT_14))
            .into(),
    ];
    if let Some(breakdown) = breakdown_label {
        header_items.push(
            container(text(breakdown).size(f32::from(tokens::FONT_10)))
                .padding([1, 6])
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(taskmanager_theme::iced::color(
                        theme_snapshot.shade,
                    ))),
                    border: iced::Border {
                        radius: 3.0.into(),
                        width: 1.0,
                        color: taskmanager_theme::iced::color(theme_snapshot.palette().border),
                    },
                    ..Default::default()
                })
                .into(),
        );
    }

    container(column(vec![
        row(header_items)
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into(),
        column(rows).spacing(8).into(),
    ]))
    .style(move |_| theme::panel_style(theme_snapshot))
    .into()
}

/// One per-core grid cell: a label, a tier-tinted rolling mini-history sparkline
/// of the core's whole window, and the latest "XX%" readout. A core with no
/// finite sample renders an honest dash — never a fabricated 0%. A core with a
/// single sample draws no polyline yet (the readout carries the value) until a
/// second snapshot arrives. The theme borrow is consumed up front (resolved into
/// `Copy` colors + an owned sample `Vec`), so the returned element is `'static`.
fn core_grid_cell(
    index: usize,
    samples: Rc<[f32]>,
    theme_snapshot: &Theme,
    core_type: Option<taskmanager_core::core::hardware::CpuType>,
) -> Elem<'static> {
    let type_suffix = match core_type {
        Some(taskmanager_core::core::hardware::CpuType::Performance) => " (P)",
        Some(taskmanager_core::core::hardware::CpuType::Efficient) => " (E)",
        Some(taskmanager_core::core::hardware::CpuType::LowPower) => " (LP)",
        _ => "",
    };
    let label = format!("C{index:02}{type_suffix}");

    let body: Elem<'static> = match samples.last().copied() {
        Some(value) => {
            let clamped = value.clamp(0.0, 100.0);
            let stroke_color = tier_color(theme_snapshot, clamped);
            let chart = canvas::Canvas::new(CoreCellChart::new(samples, stroke_color))
                .width(Length::Fill)
                .height(Length::Fixed(CELL_CHART_HEIGHT));
            column(vec![
                chart.into(),
                text(format!("{clamped:>3.0}%"))
                    .size(f32::from(tokens::FONT_10))
                    .into(),
            ])
            .spacing(2)
            .into()
        }
        None => text(MISSING_VALUE).size(f32::from(tokens::FONT_12)).into(),
    };
    column(vec![
        text(label).size(f32::from(tokens::FONT_10)).into(),
        body,
    ])
    .spacing(2)
    .width(Length::FillPortion(1))
    .into()
}

/// One logical core's rolling utilization window as a mini line+area sparkline,
/// tier-colored by the cell's latest sample. Built fresh each render from the
/// shared per-core window; read-only (produces no messages). A window with
/// fewer than two samples strokes nothing — the honest collecting state, never
/// a fabricated single-segment line.
struct CoreCellChart {
    samples: Rc<[f32]>,
    color: Color,
}

impl CoreCellChart {
    fn new(samples: impl Into<Rc<[f32]>>, color: Color) -> Self {
        Self {
            samples: samples.into(),
            color,
        }
    }

    /// The fingerprint identifying the geometry this program would draw for its
    /// current data. Pure so the cross-frame cache-clear gate is unit-tested
    /// without a renderer (see [`CoreCellFingerprint`]).
    #[must_use]
    fn fingerprint(&self) -> CoreCellFingerprint {
        CoreCellFingerprint::from_samples(&self.samples)
    }
}

/// The cached identity of one immutable per-core snapshot generation. The tier
/// color derives from the same snapshot. Color theme tokens are NOT part of
/// the fingerprint (a theme switch
/// is rare and one stale-color frame is acceptable — matches round-1
/// `process_sparkline`).
#[derive(Clone, Default, PartialEq, Debug)]
struct CoreCellFingerprint {
    samples: SeriesGeneration,
}

impl CoreCellFingerprint {
    /// Build the fingerprint for one immutable sample-window generation.
    #[must_use]
    fn from_samples(samples: &Rc<[f32]>) -> Self {
        Self {
            samples: SeriesGeneration::new(samples),
        }
    }
}

/// The persistent per-canvas state: the geometry [`Cache`] plus the last
/// fingerprint drawn into it. `Default`-derivable so iced can seed one when the
/// canvas node first appears; the `Cache` and the `Cell` both reuse interior
/// mutability so [`canvas::Program::draw`] (which takes `&State`) can clear and
/// redraw through a shared reference.
#[derive(Default)]
struct CoreCellState {
    cache: Cache,
    fingerprint: RefCell<CoreCellFingerprint>,
}

impl canvas::Program<Message> for CoreCellChart {
    type State = CoreCellState;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        // Clear the cache ONLY when the data fingerprint changed. iced's
        // `Cache::draw` then reuses last frame's Geometry whenever the bounds
        // are stable and the cache was not cleared — so the ~60 Hz repaint loop
        // recomputes the line+area only on the ~1 Hz tick that actually moved
        // the per-core window.
        let current = self.fingerprint();
        if *state.fingerprint.borrow() != current {
            *state.fingerprint.borrow_mut() = current;
            state.cache.clear();
        }
        // `Cache::draw` invokes the closure synchronously. Avoid copying each
        // core's history window when the geometry cache is already warm.
        let samples = &self.samples;
        let color = self.color;
        let geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            let size = frame.size();
            // Reuse the Performance chart's `[0,100]`→frame projection so a
            // per-core sparkline and the full chart read the same sample
            // identically.
            for points in series_point_runs(samples, size) {
                if points.len() < 2 {
                    continue;
                }
                if let Some(area) = area_path(&points, size.height) {
                    frame.fill(&area, color.scale_alpha(CELL_AREA_ALPHA));
                }
                if let Some(line) = line_path(&points) {
                    frame.stroke(
                        &line,
                        canvas::Stroke::default()
                            .with_width(CELL_STROKE_WIDTH)
                            .with_color(color),
                    );
                }
            }
        });
        vec![geometry]
    }
}

/// The load-tier color for one utilization value: success below [`WARN_EDGE`],
/// warning up to [`DANGER_EDGE`], danger above — the same green/amber/red bands
/// the TUI grid paints (both derive from the shared theme tokens).
#[must_use]
fn tier_color(theme: &Theme, utilization: f32) -> Color {
    let pct = utilization.clamp(0.0, 100.0);
    let token = if pct >= DANGER_EDGE {
        theme.danger
    } else if pct >= WARN_EDGE {
        theme.warning
    } else {
        theme.success
    };
    taskmanager_theme::iced::color(token)
}

#[cfg(test)]
#[path = "../../tests/gui/ui/core_grid_tests.rs"]
mod tests;
