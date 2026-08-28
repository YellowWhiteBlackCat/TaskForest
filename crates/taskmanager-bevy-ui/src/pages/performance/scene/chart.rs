//! Performance chart-card and grid scene builders.

use super::*;

/// Bar height from a sparkline fraction against the density-scaled strip
/// height (two standard control heights).
pub(crate) fn bar_height(fraction: f32, palette: &UiPalette) -> f32 {
    (fraction * palette.control_height_px * 2.0).max(1.0)
}

pub(crate) fn bar_scene(height_px: f32, color: bevy::color::Color) -> impl Scene + use<> {
    let fill = color.with_alpha(0.22);
    bsn! {
        Node {
            flex_grow: 1.0,
            min_width: px(1.0),
            width: px(space_2()),
            height: px(height_px),
            position_type: PositionType::Relative,
        }
        BackgroundColor(fill)
        Children [
            (
                Node {
                    width: percent(100),
                    height: px(2.0),
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    top: px(0.0),
                }
                BackgroundColor(color)
            ),
        ]
    }
}

/// The strip's initial bars, one per warm sample (none while collecting).
pub(super) fn curve_bars(
    curve: SystemCurve,
    fractions: &[f32],
    palette: &UiPalette,
) -> Vec<impl Scene + use<>> {
    let color = curve.color(palette);
    fractions
        .iter()
        .map(|fraction| bar_scene(bar_height(*fraction, palette), color))
        .collect()
}

pub(super) fn curve_card_scene(
    curve: SystemCurve,
    shell: &ShellApp,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let title = curve.title();
    let caption = curve_caption(shell, curve);
    let strip_height = palette.control_height_px * 3.0;
    let samples = curve_samples(shell, curve);
    let fractions = if curve_warm(&samples) {
        bar_fractions(&samples)
    } else {
        Vec::new()
    };
    // GPUI's compact performance surface keeps one selected hero graph in
    // view. The selector swaps this card in place; hidden cards retain their
    // markers and remain cheap to refresh when selected.
    let display = if curve == SystemCurve::default() && curve_wanted(shell, curve) {
        Display::Flex
    } else {
        Display::None
    };
    let bars = curve_bars(curve, &fractions, palette);
    let overlay =
        (!curve_warm(&samples)).then(|| collecting_overlay_scene(caption.clone(), palette));
    let segment_count = line_segments(
        &shell.history.series(curve.series()),
        100.0,
        strip_height,
        MAX_CHART_POINTS,
    )
    .len();
    bsn! {
        Node {
            flex_grow: 1.0,
            min_width: px(palette.control_height_px * 6.0),
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
            ( Text(title) TextRole(Role::Caption) ),
            (
                Node {
                    width: percent(100),
                    height: px(strip_height),
                    position_type: PositionType::Relative,
                }
                Children [
                    ( chart_grid_scene(strip_height, palette) ),
                    (
                        Node {
                            width: percent(100),
                            height: percent(100),
                            position_type: PositionType::Absolute,
                            left: px(0.0),
                            top: px(0.0),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::FlexEnd,
                            column_gap: Val::Px(space_2()),
                        }
                        SparkStrip(curve)
                        ChartSurface({ segment_count })
                        Children [
                            { bars },
                        ]
                    ),
                    ( { overlay } ),
                ]
            ),
            (
                Text(caption)
                TextRole(Role::Caption)
                DynText(DynField::CurveCaption(curve))
            ),
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
            height: px(1.0),
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
            width: px(1.0),
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
            { horizontal },
            { vertical },
        ]
    }
}

pub(super) fn collecting_overlay_scene(caption: String, palette: &UiPalette) -> Box<dyn Scene> {
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
            (
                Node {
                    padding: UiRect::horizontal(Val::Px(space_8())),
                    height: px(palette.control_height_px),
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(palette.control_radius_px)),
                }
                BackgroundColor({ palette.content_bg })
                Children [
                    ( Text(caption) TextRole(Role::Caption) ),
                ]
            ),
        ]
    })
}
