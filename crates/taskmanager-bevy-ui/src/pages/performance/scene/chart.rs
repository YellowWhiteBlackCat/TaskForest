//! Performance chart-card and grid scene builders.
//!
//! The card's curve is a continuous canvas: the strip measures the slot the
//! layout gave it, and the curve is projected against that measurement — no
//! design width, no rotated rectangles.

use super::hover::CurveSeries;
use super::*;
use crate::widgets::chart::canvas::{CANVAS_FILL_ALPHA, CanvasFill, CanvasGrid};
use crate::widgets::chart::{
    ChartCanvas, ChartContent, ChartHoverLabel, ChartPaintState, ChartPointer, ChartStyle,
    ChartSurface,
};
use taskforest_core::core::alerts::AlertMetric;
use taskmanager_shell::presentation::bytes;

/// The calibration line's thickness in px — a hairline is a real chart
/// geometry contract, not a layout length.
const THRESHOLD_LINE_PX: f32 = 1.0;
/// The calibration label's offset from the line, in px.
const THRESHOLD_LABEL_OFFSET_PX: f32 = 2.0;
/// The grid rule's thickness in px — the same hairline contract as the
/// calibration line: a grid rule is chart geometry, not a layout length.
const GRID_LINE_PX: f32 = 1.0;

/// Marker for the chart's alert calibration line.
#[derive(bevy::ecs::component::Component, Clone, Copy, Debug, Default)]
pub(crate) struct ThresholdCalibration;

/// The percentage ceiling a percent curve is projected against. The
/// calibration line rides the same scale as the polyline, so the line and
/// the curve can never disagree about where a value sits.
const PERCENT_CEILING: f32 = 100.0;

/// The grid's division count, matching the reference surface.
const GRID_DIVISIONS: usize = 5;

/// The emphasized quarter rules the reference surface draws under the curve.
const GRID_REFERENCE: &[f32] = &[0.25, 0.5, 0.75];

/// The alert metric a curve's threshold rule calibrates. Only the percent
/// curves carry an alert rule; the throughput curve has no percentage
/// threshold to mark.
fn curve_alert_metric(curve: SystemCurve) -> Option<AlertMetric> {
    match curve {
        SystemCurve::Cpu => Some(AlertMetric::CpuUsagePercent),
        SystemCurve::Memory => Some(AlertMetric::MemoryUsagePercent),
        SystemCurve::Network | SystemCurve::Gpu | SystemCurve::Npu => None,
    }
}

/// The enabled alert rule whose threshold calibrates a curve, with the
/// threshold value. A disabled rule calibrates nothing: its threshold is
/// not armed, so drawing its line would claim a bound that is not watched.
pub(crate) fn curve_threshold(shell: &ShellApp, curve: SystemCurve) -> Option<f32> {
    let metric = curve_alert_metric(curve)?;
    shell
        .projection()
        .alert_center
        .managed_rules()
        .iter()
        .find(|managed| managed.rule.metric == metric && managed.enabled)
        .map(|managed| managed.rule.threshold)
}

/// The vertical offset of the calibration line inside the strip. The line
/// rides the curve's own scale: the same ceiling, the same strip height.
pub(crate) fn threshold_line_top(strip_height_px: f32, threshold: f32) -> f32 {
    let ratio = (threshold / PERCENT_CEILING).clamp(0.0, 1.0);
    strip_height_px * (1.0 - ratio)
}

/// The calibration line: the rule's threshold marked across the curve. The
/// ink is the theme's critical severity color — the same token the alert
/// banner paints, never a frontend literal.
fn threshold_line_scene(top: f32, palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: px(THRESHOLD_LINE_PX),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(top),
        }
        BackgroundColor({ palette.danger_color })
        ThresholdCalibration
    })
}

/// The calibration line's label: the threshold value, on the line's own
/// surface so it stays legible where the curve crosses it.
fn threshold_label_scene(top: f32, label: String, palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            position_type: PositionType::Absolute,
            right: px(0.0),
            top: px(top + THRESHOLD_LABEL_OFFSET_PX),
            padding: UiRect::horizontal(px(THRESHOLD_LABEL_OFFSET_PX)),
        }
        BackgroundColor({ palette.panel_fill })
        Children [ Text(label) TextRole(Role::Caption) ]
    })
}

/// The ceiling a curve is projected against: the percentage scale for the
/// utilization curves, the measured peak for the throughput curve.
pub(crate) fn curve_ceiling(curve: SystemCurve, samples: &[f32]) -> f32 {
    match curve {
        SystemCurve::Cpu | SystemCurve::Memory | SystemCurve::Gpu | SystemCurve::Npu => {
            PERCENT_CEILING
        }
        SystemCurve::Network => {
            samples
                .iter()
                .copied()
                .filter(|sample| sample.is_finite())
                .fold(1.0_f32, f32::max)
                * 1.1
        }
    }
}

/// The percent readout the scale ruler and the value pill print.
fn percent_readout(value: f32) -> String {
    format!("{value:.0}%")
}

/// The throughput readout the scale ruler and the value pill print.
fn throughput_readout(value: f32) -> String {
    format!("{}/s", bytes(value.max(0.0) as u64))
}

/// The readout a curve's own scale is printed with.
fn curve_readout(curve: SystemCurve) -> fn(f32) -> String {
    match curve {
        SystemCurve::Network => throughput_readout,
        _ => percent_readout,
    }
}

/// The style the curve is painted with: the curve's own ink, the wash under
/// it, and the grid it sits on.
fn curve_style(curve: SystemCurve, palette: &UiPalette) -> ChartStyle {
    let color = curve.color(palette);
    ChartStyle {
        line: color,
        thickness: crate::widgets::chart::geometry::CHART_LINE_THICKNESS_PX,
        fill: Some(CanvasFill {
            color,
            alpha: CANVAS_FILL_ALPHA,
        }),
        flow: None,
        grid: Some(CanvasGrid {
            color: palette.dim_color,
            alpha: 0.13,
            horizontal: GRID_DIVISIONS,
            vertical: GRID_DIVISIONS + 1,
            reference: GRID_REFERENCE,
            reference_alpha: 0.26,
        }),
        format: Some(curve_readout(curve)),
    }
}

/// The value pill: the hovered sample's readout, riding on the canvas.
fn hover_pill_scene(palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            display: Display::None,
            padding: UiRect::horizontal(Val::Px(space_2())),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        ChartHoverLabel
        Children [ Text(String::new()) TextRole(Role::Caption) ]
    })
}

/// The strip's canvas: the continuous curve, its wash, and the grid.
fn curve_canvas_scene(
    curve: SystemCurve,
    samples: Vec<f32>,
    ceiling: f32,
    style: ChartStyle,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let points = samples.len();
    let content = ChartContent::Samples {
        values: samples,
        ceiling,
    };
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            overflow: Overflow::clip(),
        }
        ChartCanvas { content: {content}, style: {style} }
        SparkStrip(curve)
        ChartSurface({ points })
        ChartPointer
        ChartPaintState
        CurveSeries(curve)
        Children [ @{ hover_pill_scene(palette) } ]
    }
}

/// The scale ruler's labels: the curve's own scale, read top to bottom.
pub(crate) fn ruler_labels(curve: SystemCurve, ceiling: f32) -> Vec<String> {
    let readout = curve_readout(curve);
    [1.0_f32, 0.75, 0.5, 0.25, 0.0]
        .iter()
        .map(|fraction| readout(ceiling * fraction))
        .collect()
}

/// The scale ruler beside the strip: the percentages (or rates) the curve's
/// own scale marks.
fn ruler_scene(curve: SystemCurve, ceiling: f32, palette: &UiPalette) -> impl Scene + use<> {
    let labels: Vec<Box<dyn Scene>> = ruler_labels(curve, ceiling)
        .into_iter()
        .map(|label| {
            Box::new(bsn! {
                Text(label)
                TextRole(Role::Caption)
                TextLayout { linebreak: LineBreak::NoWrap }
            }) as Box<dyn Scene>
        })
        .collect();
    bsn! {
        Node {
            width: rem(palette.control_height_rem * 3.0),
            height: percent(100),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::FlexEnd,
        }
        Children [ { labels } ]
    }
}

pub(crate) fn curve_card_scene(
    curve: SystemCurve,
    shell: &ShellApp,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let title = curve.title();
    let caption = curve_caption(shell, curve);
    // The strip budget rides the rem axis: the scene declares the rem ratio
    // and the curve resolves the same budget in px against the live rem size,
    // so the chart and its geometry can never disagree.
    let strip_height_rem = palette.control_height_rem * 3.0;
    let strip_height_px = strip_height_rem * palette.rem_px;
    let samples = curve_samples(shell, curve);
    let warm = curve_warm(&samples);
    let ceiling = curve_ceiling(curve, &samples);
    let style = curve_style(curve, palette);
    // GPUI's compact performance surface keeps one selected hero graph in
    // view. The selector swaps this card in place; hidden cards retain their
    // markers and remain cheap to refresh when selected.
    let display = if curve == SystemCurve::default() && curve_wanted(shell, curve) {
        Display::Flex
    } else {
        Display::None
    };
    let overlay = (!warm).then(|| collecting_overlay_scene(caption.clone(), palette));
    // The NPU card carries the accelerator's power curve beside its compute
    // curve. The section exists only while the accelerator REPORTS the power
    // fact: an absent capability leaves the card with the compute curve
    // alone, never a flat zero line standing in for the energy series.
    let power = if curve == SystemCurve::Npu {
        super::npu_power::section(shell, palette)
    } else {
        None
    };
    // The alert threshold calibrates the curve: an enabled rule for the
    // curve's metric marks its bound across the strip, on the curve's own
    // scale.
    let threshold = curve_threshold(shell, curve);
    let calibration: Vec<Box<dyn Scene>> = match threshold {
        Some(threshold) => {
            let top = threshold_line_top(strip_height_px, threshold);
            vec![
                threshold_line_scene(top, palette),
                threshold_label_scene(top, format!("≥{threshold:.0}%"), palette),
            ]
        }
        None => Vec::new(),
    };
    bsn! {
        Node {
            flex_grow: 1.0,
            min_width: rem(palette.control_height_rem * 6.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Px(space_2())),
            display: display,
            border_radius: BorderRadius::all(Val::Px(palette.panel_radius_px)),
        }
        BackgroundColor({ palette.panel_fill })
        CurveCard(curve)
        CurveGate(curve)
        Children [
             Text(title) TextRole(Role::Caption) --

                Node {
                    width: percent(100),
                    height: rem(strip_height_rem),
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(space_2()),
                    position_type: PositionType::Relative,
                    overflow: Overflow::clip_x(),
                }
                Children [
                     Node {
                         flex_grow: 1.0,
                         min_width: px(0.0),
                         height: percent(100),
                         position_type: PositionType::Relative,
                         overflow: Overflow::clip_x(),
                     }
                     Children [
                          @{ curve_canvas_scene(curve, samples, ceiling, style, palette) } --
                          { calibration } --
                          @{ overlay }
                     ]
                 --
                     @{ ruler_scene(curve, ceiling, palette) }
                ]
            --
            @{ power } --

                Text(caption)
                TextRole(Role::Caption)
                DynText(DynField::CurveCaption(curve))

        ]
    }
}

pub(super) fn grid_line_color(palette: &UiPalette) -> bevy::color::Color {
    palette.dim_color.with_alpha(0.18)
}

pub(super) fn horizontal_grid_line(top: f32, palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            height: px(GRID_LINE_PX),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(top),
        }
        BackgroundColor({ grid_line_color(palette) })
    }
}

pub(super) fn vertical_grid_line(left: f32, palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node {
            width: px(GRID_LINE_PX),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: percent(left),
            top: px(0.0),
        }
        BackgroundColor({ grid_line_color(palette) })
    }
}

pub(super) fn chart_grid_scene(height: f32, palette: &UiPalette) -> impl Scene + use<> {
    let horizontal: Vec<Box<dyn Scene>> = (1..=4)
        .map(|index| {
            Box::new(horizontal_grid_line(height * index as f32 / 5.0, palette)) as Box<dyn Scene>
        })
        .collect();
    let vertical: Vec<Box<dyn Scene>> = (1..=5)
        .map(|index| {
            Box::new(vertical_grid_line(index as f32 / 6.0 * 100.0, palette)) as Box<dyn Scene>
        })
        .collect();
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
        }
        Children [
            { horizontal }--
            { vertical }
        ]
    }
}

pub(crate) fn collecting_overlay_scene(caption: String, palette: &UiPalette) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
        }
        Children [

                Node {
                    padding: UiRect::horizontal(Val::Px(space_8())),
                    height: rem(palette.control_height_rem),
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                }
                BackgroundColor({ palette.content_bg })
                Children [
                     Text(caption) TextRole(Role::Caption)
                ]

        ]
    })
}

#[cfg(test)]
#[path = "../../../../tests/headless/pages/alerts/threshold.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/headless/pages/chart_parity.rs"]
mod parity_tests;
