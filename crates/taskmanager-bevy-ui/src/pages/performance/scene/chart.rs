//! Performance chart-card and grid scene builders.
//!
//! The card's curve is a continuous canvas: the strip measures the slot the
//! layout gave it, and the curve is projected against that measurement — no
//! design width, no rotated rectangles.

use super::hover::CurveSeries;
use super::*;
use crate::glass::{
    GlassPanelMaterial, OpticalBackdropSource, OpticalGlassCard, card_material, surface_material,
};
use crate::widgets::chart::canvas::{CANVAS_FILL_ALPHA, CanvasFill, CanvasGrid};
use crate::widgets::chart::{
    ChartCanvas, ChartContent, ChartHoverLabel, ChartPaintState, ChartPointer, ChartStyle,
    ChartSurface,
};
use crate::window::CustomTextInk;
use bevy::picking::Pickable;
use bevy::text::TextColor;
use taskforest_core::core::percent::percent_text;
use taskmanager_shell::presentation::GraphSummaryItem;
use taskmanager_shell::presentation::bytes;
use taskmanager_theme::{GlassSurfaceRole, GlassTier};

/// The value badge's edge inset and padding, matching the GPUI chart.
const CURVE_BADGE_INSET_PX: f32 = 2.0;
const CURVE_BADGE_PAD_X_PX: f32 = 4.0;
const CURVE_BADGE_PAD_Y_PX: f32 = 1.5;
const CURVE_BADGE_WASH_ALPHA: f32 = 0.18;
/// The grid rule's thickness in px — the same hairline contract as the
/// curve: a grid rule is chart geometry, not a layout length.
const GRID_LINE_PX: f32 = 1.0;

use taskforest_core::core::percent::MAX_PERCENT;

/// The grid's division count, matching the reference surface.
const GRID_DIVISIONS: usize = 5;

/// The wash's peak alpha under the curve: the reference surface's gradient
/// starts at the series ink's own 35% right below the curve and fades to
/// nothing at the floor. The measured reference pixels resolve to ~0.37 at
/// the curve's edge, so the shared canvas constant IS the curve's value.
const CURVE_FILL_ALPHA: f32 = CANVAS_FILL_ALPHA;

/// The emphasized quarter rules the reference surface draws under the curve.
const GRID_REFERENCE: &[f32] = &[0.25, 0.5, 0.75];

/// The summary pill's insets and bound. The reference surface pins the
/// latest/avg/peak readout to the card's BOTTOM-LEFT corner inside a real
/// safety band — it never rides the rounded border, and it never stacks a row
/// under the plot that would steal the curve's vertical budget.
const SUMMARY_BOTTOM_INSET_PX: f32 = 6.0;
const SUMMARY_LEFT_INSET_PX: f32 = 8.0;
const SUMMARY_MAX_WIDTH_PX: f32 = 320.0;

/// The ceiling a curve is projected against: the percentage scale for the
/// utilization curves, the measured peak for the throughput curve.
pub(crate) fn curve_ceiling(curve: SystemCurve, samples: &[f32]) -> f32 {
    match curve {
        SystemCurve::Cpu | SystemCurve::Memory | SystemCurve::Gpu | SystemCurve::Npu => MAX_PERCENT,
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

/// The percent readout the hover pill and value badge print.
fn percent_readout(value: f32) -> String {
    percent_text(value)
}

/// The throughput readout the hover pill and value badge print.
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
            alpha: CURVE_FILL_ALPHA,
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

/// The hovered sample's readout, riding on the canvas.
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
///
/// The canvas carries the window's **slot capacity**: the frame represents
/// that many samples, so a window holding fewer samples grows from the right
/// edge instead of claiming the whole frame. The capacity is the shell's own
/// history window — the same `graph_data_points` preference the other
/// frontends read — so every frontend paints the same span.
fn curve_canvas_scene(
    curve: SystemCurve,
    samples: Vec<f32>,
    ceiling: f32,
    capacity: usize,
    style: ChartStyle,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let points = samples.len();
    let content = ChartContent::Samples {
        values: samples,
        ceiling,
        capacity,
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
        OpticalBackdropSource {
            enabled: {curve == SystemCurve::default()},
            grid: {style.grid},
        }
        CurveSeries(curve)
        Children [ @{ hover_pill_scene(palette) } ]
    }
}

/// The newest sample's pill: the GPUI chart's always-on readout. It stays
/// absent until a finite observation exists, so gaps never become values.
fn curve_badge_scene(curve: SystemCurve, shell: &ShellApp, palette: &UiPalette) -> Box<dyn Scene> {
    let ink = curve.color(palette);
    let wash = ink.with_alpha(CURVE_BADGE_WASH_ALPHA);
    let latest = curve_badge_text(shell, curve);
    let display = if latest.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    let text = latest.unwrap_or_default();
    Box::new(bsn! {
        Node {
            position_type: PositionType::Absolute,
            right: px(CURVE_BADGE_INSET_PX),
            top: px(CURVE_BADGE_INSET_PX),
            display: display,
            padding: UiRect::axes(
                px(CURVE_BADGE_PAD_X_PX),
                px(CURVE_BADGE_PAD_Y_PX),
            ),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        CurveBadge({ curve })
        BackgroundColor({ wash })
        Children [
            Text(text)
            TextRole(Role::Caption)
            TextColor({ ink })
            CustomTextInk
            TextLayout { linebreak: LineBreak::NoWrap }
            DynText(DynField::CurveBadge({ curve }))
        ]
    })
}

/// The chart's latest/avg/peak readout, overlaid on the card's bottom-left
/// corner. It is a glass pill in its own right — the card material gives it
/// the same edge and radius contract the reference surface uses — so it reads
/// as a surface, not as loose text floating over the grid, and it occludes the
/// plot behind it instead of competing with it.
fn summary_pill_scene(curve: SystemCurve, shell: &ShellApp, palette: &UiPalette) -> Box<dyn Scene> {
    let role = GlassSurfaceRole::InlinePlot;
    let GlassPanelMaterial {
        fill,
        edge,
        shadows: _,
        edge_width_rem,
        radius_rem: _,
        grain: _,
    } = surface_material(role, palette);
    let items = [
        GraphSummaryItem::Latest,
        GraphSummaryItem::Average,
        GraphSummaryItem::Maximum,
    ]
    .into_iter()
    .map(|item| {
        let caption = super::super::metrics::curve_caption_item(shell, curve, item);
        Box::new(bsn! {
            Text(caption)
            TextRole(Role::Caption)
            TextLayout { linebreak: LineBreak::NoWrap }
            DynText(DynField::CurveCaptionItem(curve, item))
        }) as Box<dyn Scene>
    })
    .collect::<Vec<_>>();
    Box::new(bsn! {
        Node {
            position_type: PositionType::Absolute,
            bottom: px(SUMMARY_BOTTOM_INSET_PX),
            left: px(SUMMARY_LEFT_INSET_PX),
            max_width: px(SUMMARY_MAX_WIDTH_PX),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(space_12()),
            padding: UiRect::axes(px(space_8()), px(space_2())),
            border: UiRect::all(Val::Rem(edge_width_rem)),
            border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
        }
        BackgroundColor({ fill })
        BorderColor::all(edge)
        Pickable::IGNORE
        Children [{ items }]
    })
}

pub(crate) fn curve_card_scene(
    curve: SystemCurve,
    shell: &ShellApp,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let caption = curve_caption(shell, curve);
    // The strip budget rides the rem axis: the scene declares the rem ratio
    // and the curve resolves the same budget in px against the live rem size,
    // so the chart and its geometry can never disagree.
    let strip_height_rem = palette.control_height_rem * 10.0;
    let samples = curve_samples(shell, curve);
    let warm = curve_warm(&samples);
    let ceiling = curve_ceiling(curve, &samples);
    // The frame's slot capacity: the shell's own history window, which the
    // persisted `graph_data_points` preference sets. Every frontend reads the
    // same window, so every frontend paints the same span.
    let capacity = shell.history.capacity();
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
    let badge = curve_badge_scene(curve, shell, palette);
    let GlassPanelMaterial {
        fill,
        edge,
        shadows: _,
        edge_width_rem,
        radius_rem,
        grain,
    } = card_material(palette);
    bsn! {
        Node {
            flex_grow: 1.0,
            min_width: rem(palette.control_height_rem * 6.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(space_2()),
            padding: UiRect::all(Val::Rem(palette.panel_content_inset_rem())),
            display: display,
            border: UiRect::all(Val::Rem(edge_width_rem)),
            border_radius: BorderRadius::all(Val::Rem(radius_rem)),
        }
        BackgroundColor({ fill })
        BorderColor::all(edge)
        OpticalGlassCard {
            tier: {GlassTier::Card},
            enabled: {curve == SystemCurve::default()},
        }
        CurveCard(curve)
        CurveGate(curve)
        Children [
             @{ grain } --
                Node {
                    width: percent(100),
                    flex_grow: 1.0,
                    flex_basis: px(0.0),
                    min_height: rem(strip_height_rem),
                    position_type: PositionType::Relative,
                    overflow: Overflow::clip(),
                }
                Children [
                     @{ curve_canvas_scene(curve, samples, ceiling, capacity, style, palette) } --
                     @{ badge } --
                     @{ overlay }
                ]
            --
            @{ power } --
            @{ summary_pill_scene(curve, shell, palette) }
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
