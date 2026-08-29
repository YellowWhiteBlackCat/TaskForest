//! Performance chart-card and grid scene builders.

use super::*;

/// Design width of the curve strip in px. bevy_ui flex cannot report a
/// computed node width to the fold observer (the same M1 constraint as the
/// process table's viewport), so the polyline projects against this fixed
/// design width — sized to the default 1180px window's card interior so the
/// NEWEST sample stays inside the clip on the primary surface; narrower
/// windows clip the oldest tail instead of the live edge. The chart contract
/// pairs it with [`MAX_CHART_POINTS`].
pub(crate) const CHART_STRIP_WIDTH_PX: f32 = 450.0;

pub(crate) fn curve_card_scene(
    curve: SystemCurve,
    shell: &ShellApp,
    palette: &UiPalette,
) -> impl Scene + use<> {
    let title = curve.title();
    let caption = curve_caption(shell, curve);
    let strip_height = palette.control_height_px * 3.0;
    let samples = curve_samples(shell, curve);
    // The polyline is the only curve render path (M2): the bounded,
    // gap-aware projection feeds rotated 2px segments — the same visual
    // grammar as GPUI's graphs, honest gaps included.
    let segments = if curve_warm(&samples) {
        line_segments(
            &taskmanager_shell::presentation::trend::window(&shell.history, curve.series()),
            CHART_STRIP_WIDTH_PX,
            strip_height,
            MAX_CHART_POINTS,
        )
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
    let color = curve.color(palette);
    let overlay =
        (!curve_warm(&samples)).then(|| collecting_overlay_scene(caption.clone(), palette));
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
                    overflow: Overflow::clip_x(),
                }
                Children [
                    ( chart_grid_scene(strip_height, palette) ),
                    (
                        Node {
                            width: px(CHART_STRIP_WIDTH_PX),
                            height: percent(100),
                            position_type: PositionType::Absolute,
                            left: px(0.0),
                            top: px(0.0),
                        }
                        SparkStrip(curve)
                        ChartSurface({ segments.len() })
                        Children [
                            ( { polyline_scene(&segments, color) } ),
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
