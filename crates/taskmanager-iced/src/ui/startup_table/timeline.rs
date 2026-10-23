//! Startup boot timeline and waterfall block for Iced.
//!
//! The waterfall is a real gantt over the boot's measured span (BN-05 +
//! ICED-044): every unit window is PLACED at its activation offset and SIZED
//! by its duration share, the axis carries tick lines at "nice" time steps,
//! and the unit labels are staggered into lanes so two units that start
//! within the same instant never wear their names on top of each other.
//! Every position comes from the measured values — the activation offsets,
//! the durations and the shaped text's own measured width — never from an
//! estimate.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use taskmanager_application::boot_timeline_rows;
use taskmanager_application::i18n::t;
use taskmanager_core::core::startup::StartupBootEvidenceSnapshot;

use taskmanager_theme::tokens;

use crate::app::Message;
use crate::text_metrics::{measured_text_width, truncate_to_width};
use crate::theme;
use taskmanager_theme::Theme;

/// Waterfall bar track width in px in standard layout (a layout contract, not a theme token).
pub(crate) const TIMELINE_BAR_WIDTH: f32 = 220.0;
/// Waterfall bar track width in px in compact layout to avoid overflowing narrow viewports.
pub(crate) const TIMELINE_BAR_WIDTH_COMPACT: f32 = 130.0;

/// Unit name column width in px in standard layout.
pub(crate) const TIMELINE_UNIT_WIDTH: f32 = 190.0;
/// Unit name column width in px in compact layout.
pub(crate) const TIMELINE_UNIT_WIDTH_COMPACT: f32 = 130.0;

/// Minimum visible bar width so a 0-duration activation is still a mark.
pub(crate) const TIMELINE_MIN_BAR_PX: f32 = 3.0;

/// Duration readout column width in px in standard layout.
pub(crate) const TIMELINE_DURATION_WIDTH: f32 = 70.0;
/// Duration readout column width in px in compact layout.
pub(crate) const TIMELINE_DURATION_WIDTH_COMPACT: f32 = 56.0;

/// The gantt track width in px in standard layout (the measured boot span
/// scaled onto this band).
pub(crate) const TIMELINE_TRACK_WIDTH: f32 = 480.0;
/// The gantt track width in px in compact layout.
pub(crate) const TIMELINE_TRACK_WIDTH_COMPACT: f32 = 316.0;
/// How many tick lines the axis aims for (a layout contract, not a token).
pub(crate) const TIMELINE_TICK_TARGET: usize = 5;
/// The tick line's height in px.
pub(crate) const TIMELINE_TICK_HEIGHT: f32 = 8.0;
/// The slot one tick's time label occupies (the label is centered in it).
pub(crate) const TIMELINE_TICK_LABEL_SLOT: f32 = 52.0;

/// One renderable waterfall row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TimelineRowKind {
    Measured {
        unit: String,
        fraction: f32,
        duration_ms: u64,
    },
    Untimed {
        count: usize,
        names: Vec<String>,
    },
    Collapsed {
        count: usize,
    },
}

/// Format an impact duration cleanly: milliseconds when under 1 second,
/// fractional seconds when under 1 minute, and minutes + seconds for longer spans.
#[must_use]
pub(crate) fn format_impact_time(duration_ms: u64) -> String {
    if duration_ms >= 60_000 {
        let mins = duration_ms / 60_000;
        let rem_s = (duration_ms % 60_000) as f64 / 1000.0;
        if duration_ms.is_multiple_of(60_000) {
            format!("{mins} min")
        } else {
            format!("{mins}m {rem_s:.1}s")
        }
    } else if duration_ms >= 1_000 {
        let secs = duration_ms as f64 / 1000.0;
        if duration_ms.is_multiple_of(1_000) {
            format!("{secs:.0} s")
        } else if duration_ms.is_multiple_of(100) {
            format!("{secs:.1} s")
        } else {
            format!("{secs:.2} s")
        }
    } else {
        format!("{duration_ms} ms")
    }
}

/// Pure waterfall projection over one typed evidence snapshot, or `None`
/// when the block must stay silent (no evidence / typed failure).
pub(crate) fn startup_timeline(
    evidence: Option<&StartupBootEvidenceSnapshot>,
) -> Option<(u64, Vec<TimelineRowKind>)> {
    let timeline = boot_timeline_rows(evidence?)?;
    let mut rows: Vec<TimelineRowKind> = timeline
        .segments
        .iter()
        .map(|segment| TimelineRowKind::Measured {
            unit: segment.unit.clone(),
            fraction: timeline.fraction_of_total(segment),
            duration_ms: segment.duration_ms,
        })
        .collect();
    if timeline.untimed_count > 0 {
        rows.push(TimelineRowKind::Untimed {
            count: timeline.untimed_count,
            names: timeline.untimed_units.clone(),
        });
    }
    if timeline.collapsed_count > 0 {
        rows.push(TimelineRowKind::Collapsed {
            count: timeline.collapsed_count,
        });
    }
    Some((timeline.total_ms, rows))
}

/// The display-only waterfall block; `None` when silent.
pub(crate) fn boot_timeline_block<'a>(
    theme_snapshot: &'a Theme,
    evidence: Option<&StartupBootEvidenceSnapshot>,
    compact: bool,
) -> Option<Element<'a, Message, iced::Theme, iced::Renderer>> {
    let (total_ms, kinds) = startup_timeline(evidence)?;
    let muted = theme::muted_text_color(theme_snapshot);
    let accent = crate::theme_binding::color(theme_snapshot.accent);
    let warning_color = crate::theme_binding::color(theme_snapshot.palette().warning);
    let track = crate::theme_binding::color(theme_snapshot.card_surface());

    let (bar_width, unit_width, duration_width, bar_height, spacing, padding, row_spacing) =
        if compact {
            (
                TIMELINE_BAR_WIDTH_COMPACT,
                TIMELINE_UNIT_WIDTH_COMPACT,
                TIMELINE_DURATION_WIDTH_COMPACT,
                6.0,
                6.0,
                4.0,
                2.0,
            )
        } else {
            (
                TIMELINE_BAR_WIDTH,
                TIMELINE_UNIT_WIDTH,
                TIMELINE_DURATION_WIDTH,
                8.0,
                8.0,
                8.0,
                2.0,
            )
        };

    let font_size = if compact {
        f32::from(tokens::FONT_10)
    } else {
        f32::from(tokens::FONT_11)
    };

    let header_size = if compact {
        f32::from(tokens::FONT_11)
    } else {
        f32::from(tokens::FONT_12)
    };

    let track_width = if compact {
        TIMELINE_TRACK_WIDTH_COMPACT
    } else {
        TIMELINE_TRACK_WIDTH
    };
    // The gantt band: the measured windows placed on the axis, their labels
    // staggered into lanes, and the axis' own tick lines above them.
    let gantt = timeline_gantt(evidence, track_width, font_size)?;

    let mut rows: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = vec![
        row![
            text(t("startup.timeline"))
                .size(header_size)
                .wrapping(iced::widget::text::Wrapping::None),
            text(format_impact_time(total_ms))
                .size(font_size)
                .color(muted)
                .wrapping(iced::widget::text::Wrapping::None),
        ]
        .spacing(spacing)
        .align_y(iced::Alignment::Center)
        .into(),
        timeline_axis(theme_snapshot, &gantt.ticks, track_width, font_size),
        timeline_lanes(&gantt, bar_height, font_size, accent, warning_color, track),
    ];
    for kind in kinds {
        let element: Element<'a, Message, iced::Theme, iced::Renderer> = match kind {
            // The measured windows are drawn by the gantt band above (the
            // row projection keeps their text for the shared vocabulary).
            TimelineRowKind::Measured { .. } => continue,
            TimelineRowKind::Untimed { count, names } => {
                let untimed_label =
                    truncate_to_width(t("startup.timeline_untimed"), unit_width, font_size);
                let untimed_raw = format!("{count} · {}", names.join(" · "));
                let detail_budget = bar_width + spacing + duration_width;
                let untimed_detail = truncate_to_width(&untimed_raw, detail_budget, font_size);
                row![
                    text(untimed_label)
                        .size(font_size)
                        .color(muted)
                        .width(Length::Fixed(unit_width))
                        .wrapping(iced::widget::text::Wrapping::None),
                    text(untimed_detail)
                        .size(font_size)
                        .color(muted)
                        .width(Length::Fixed(detail_budget))
                        .wrapping(iced::widget::text::Wrapping::None),
                ]
                .spacing(spacing)
                .align_y(iced::Alignment::Center)
                .into()
            }
            TimelineRowKind::Collapsed { count } => text(format!("+{count}"))
                .size(font_size)
                .color(muted)
                .wrapping(iced::widget::text::Wrapping::None)
                .into(),
        };
        rows.push(element);
    }
    Some(column(rows).spacing(row_spacing).padding(padding).into())
}

// ── the gantt band: tick lines, placed windows, staggered labels ────────────

/// One tick line on the boot time axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TimelineTick {
    /// The tick's time in milliseconds from boot.
    pub at_ms: u64,
    /// The tick's position on the axis (0..1 of the measured span).
    pub fraction: f32,
}

/// One measured unit window placed on the boot time axis.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimelineWindow {
    /// The unit's name.
    pub unit: String,
    /// The activation offset from boot.
    pub start_ms: u64,
    /// The window's own duration.
    pub duration_ms: u64,
    /// The bar's left edge on the axis (px).
    pub bar_left: f32,
    /// The bar's width (px).
    pub bar_width: f32,
}

/// One unit label placed on the boot time axis.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TimelineLabel {
    /// The unit's name, truncated to the space the axis has left.
    pub text: String,
    /// The lane the label sits in: lane 0 is the base lane, the higher lanes
    /// are the staggered ones that keep overlapping names apart.
    pub lane: usize,
    /// The label's left edge on the axis (px).
    pub left: f32,
    /// The label's MEASURED width (px).
    pub width: f32,
}

/// The gantt projection of the boot timeline.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BootGantt {
    /// The measured span the axis scales against.
    pub total_ms: u64,
    /// The axis' tick lines.
    pub ticks: Vec<TimelineTick>,
    /// The measured windows.
    pub windows: Vec<TimelineWindow>,
    /// The unit labels, each with its lane.
    pub labels: Vec<TimelineLabel>,
    /// How many lanes the labels occupy.
    pub lanes: usize,
}

/// The gantt projection over one typed evidence snapshot, or `None` when the
/// block must stay silent (no evidence / typed failure).
///
/// Every position is measured: the bars scale the activation offset and the
/// duration against the measured span, and the labels carry the shaped text's
/// own measured width.
#[must_use]
pub(crate) fn timeline_gantt(
    evidence: Option<&StartupBootEvidenceSnapshot>,
    track_width: f32,
    font_size: f32,
) -> Option<BootGantt> {
    let timeline = boot_timeline_rows(evidence?)?;
    let total_ms = timeline.total_ms;
    let ticks = timeline_ticks(
        total_ms,
        timeline_tick_interval(total_ms, TIMELINE_TICK_TARGET),
    );

    let mut windows: Vec<TimelineWindow> = Vec::with_capacity(timeline.segments.len());
    let mut labels: Vec<TimelineLabel> = Vec::with_capacity(timeline.segments.len());
    for segment in &timeline.segments {
        let bar_left = share_of_total(segment.start_ms, total_ms) * track_width;
        let bar_width =
            (share_of_total(segment.duration_ms, total_ms) * track_width).max(TIMELINE_MIN_BAR_PX);
        // The label starts where the window starts and may use the rest of
        // the track; a label that would run past the axis is ellipsized to
        // the space that is actually left. The label carries the window's
        // own duration, so the gantt band never loses the measured fact the
        // old duration column carried.
        let budget = (track_width - bar_left).max(0.0);
        let label_text = format!(
            "{} · {}",
            segment.unit,
            format_impact_time(segment.duration_ms)
        );
        let text = truncate_to_width(&label_text, budget, font_size);
        let width = measured_text_width(&text, font_size);
        // A label never leaves the track: the measured width can exceed the
        // leftover space (an ellipsis at the very edge), so the label is
        // pulled back to stay inside the axis.
        let left = bar_left.min((track_width - width).max(0.0));
        windows.push(TimelineWindow {
            unit: segment.unit.clone(),
            start_ms: segment.start_ms,
            duration_ms: segment.duration_ms,
            bar_left,
            bar_width,
        });
        labels.push(TimelineLabel {
            text,
            lane: 0,
            left,
            width,
        });
    }
    let lanes = timeline_lane_assignment(&windows, &labels);
    let lane_count = lanes.iter().copied().max().map_or(0, |lane| lane + 1);
    for (label, lane) in labels.iter_mut().zip(lanes) {
        label.lane = lane;
    }
    Some(BootGantt {
        total_ms,
        ticks,
        windows,
        labels,
        lanes: lane_count,
    })
}

/// One measured quantity's share of the axis total, as a `0.0..=1.0` ratio.
///
/// A zero total yields `0.0` for every window: there is nothing to scale by,
/// and a full-width bar would be a fabricated measurement.
fn share_of_total(value_ms: u64, total_ms: u64) -> f32 {
    if total_ms == 0 {
        return 0.0;
    }
    (value_ms as f32 / total_ms as f32).clamp(0.0, 1.0)
}

/// The axis' tick interval: a "nice" 1/2/5 × 10ⁿ step that keeps the tick
/// count near the target without inventing a fractional millisecond.
fn timeline_tick_interval(total_ms: u64, target_ticks: usize) -> u64 {
    if total_ms == 0 || target_ticks == 0 {
        return 0;
    }
    let rough = (total_ms as f64 / target_ticks as f64).max(1.0);
    let magnitude = 10_f64.powf(rough.log10().floor());
    let normalized = rough / magnitude;
    let step = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    ((step * magnitude).round() as u64).max(1)
}

/// The axis' tick lines: the interval's steps across the measured span,
/// each with its position on the axis.
fn timeline_ticks(total_ms: u64, interval_ms: u64) -> Vec<TimelineTick> {
    if total_ms == 0 || interval_ms == 0 {
        return Vec::new();
    }
    let mut ticks = Vec::new();
    let mut at_ms = 0_u64;
    while at_ms <= total_ms {
        ticks.push(TimelineTick {
            at_ms,
            fraction: share_of_total(at_ms, total_ms),
        });
        at_ms = at_ms.saturating_add(interval_ms);
    }
    ticks
}

/// Assign every window a lane so that no two windows in ONE lane overlap:
/// the greedy interval coloring over the occupied spans.
///
/// The occupied span covers both the label and the bar, so a lane never
/// carries two names on top of each other AND never carries two bars on top
/// of each other. The lane count is the measured overlap depth — a data
/// fact, never a fixed guess.
fn timeline_lane_assignment(windows: &[TimelineWindow], labels: &[TimelineLabel]) -> Vec<usize> {
    let mut lane_ends: Vec<f32> = Vec::new();
    let mut lanes: Vec<usize> = Vec::with_capacity(windows.len());
    for (window, label) in windows.iter().zip(labels) {
        let left = label.left.min(window.bar_left);
        let right = (label.left + label.width).max(window.bar_left + window.bar_width);
        let lane = match lane_ends.iter().position(|end| *end <= left) {
            Some(lane) => lane,
            None => {
                lane_ends.push(0.0);
                lane_ends.len() - 1
            }
        };
        lane_ends[lane] = right;
        lanes.push(lane);
    }
    lanes
}

/// A row of items placed at absolute x positions inside one band: every item
/// is preceded by the gap that reaches its own x, so the items keep the
/// positions the projection measured.
fn positioned_row<'a>(
    items: Vec<(f32, f32, Element<'a, Message, iced::Theme, iced::Renderer>)>,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let mut row = row![];
    let mut cursor = 0.0_f32;
    for (x, width, element) in items {
        if x > cursor {
            row = row.push(iced::widget::Space::new().width(Length::Fixed(x - cursor)));
        }
        row = row.push(element);
        cursor = x + width;
    }
    row.into()
}

/// The axis strip: the tick lines over their time labels.
fn timeline_axis<'a>(
    theme_snapshot: &'a Theme,
    ticks: &[TimelineTick],
    track_width: f32,
    font_size: f32,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    let tick_color = crate::theme_binding::color(theme_snapshot.palette().border);
    let lines: Vec<(f32, f32, Element<'a, Message, iced::Theme, iced::Renderer>)> = ticks
        .iter()
        .map(|tick| {
            let x = (tick.fraction * track_width).round();
            (
                x,
                1.0,
                container(iced::widget::Space::new())
                    .width(Length::Fixed(1.0))
                    .height(Length::Fixed(TIMELINE_TICK_HEIGHT))
                    .style(move |_| theme::fill_style(tick_color))
                    .into(),
            )
        })
        .collect();
    let labels: Vec<(f32, f32, Element<'a, Message, iced::Theme, iced::Renderer>)> = ticks
        .iter()
        .map(|tick| {
            let x = (tick.fraction * track_width - TIMELINE_TICK_LABEL_SLOT * 0.5).max(0.0);
            (
                x,
                TIMELINE_TICK_LABEL_SLOT,
                container(
                    text(format_impact_time(tick.at_ms))
                        .size(font_size)
                        .color(muted)
                        .wrapping(iced::widget::text::Wrapping::None),
                )
                .width(Length::Fixed(TIMELINE_TICK_LABEL_SLOT))
                .align_x(iced::Alignment::Center)
                .into(),
            )
        })
        .collect();
    column![positioned_row(lines), positioned_row(labels)]
        .spacing(2)
        .into()
}

/// The gantt band: one lane row per lane, each carrying the lane's labels
/// above its bars.
fn timeline_lanes<'a>(
    gantt: &BootGantt,
    bar_height: f32,
    font_size: f32,
    accent: iced::Color,
    warning_color: iced::Color,
    track: iced::Color,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let mut lanes: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = Vec::new();
    for lane in 0..gantt.lanes {
        let label_items: Vec<(f32, f32, Element<'a, Message, iced::Theme, iced::Renderer>)> = gantt
            .labels
            .iter()
            .filter(|label| label.lane == lane)
            .map(|label| {
                (
                    label.left,
                    label.width,
                    text(label.text.clone())
                        .size(font_size)
                        .width(Length::Fixed(label.width))
                        .wrapping(iced::widget::text::Wrapping::None)
                        .into(),
                )
            })
            .collect();
        let bar_items: Vec<(f32, f32, Element<'a, Message, iced::Theme, iced::Renderer>)> = gantt
            .windows
            .iter()
            .zip(gantt.labels.iter())
            .filter(|(_, label)| label.lane == lane)
            .map(|(window, _)| {
                let bar_color = if window.duration_ms > 1000 {
                    warning_color
                } else {
                    accent
                };
                (
                    window.bar_left,
                    window.bar_width,
                    container(
                        container(iced::widget::Space::new())
                            .width(Length::Fixed(window.bar_width))
                            .height(Length::Fixed(bar_height))
                            .style(move |_| theme::fill_style(bar_color)),
                    )
                    .width(Length::Fixed(window.bar_width))
                    .height(Length::Fixed(bar_height))
                    .style(move |_| theme::fill_style(track))
                    .into(),
                )
            })
            .collect();
        lanes.push(
            column![positioned_row(label_items), positioned_row(bar_items)]
                .spacing(2)
                .into(),
        );
    }
    column(lanes).spacing(4).into()
}

#[cfg(test)]
#[path = "../../../tests/gui/ui/startup_timeline_gantt_tests.rs"]
mod gantt_tests;
