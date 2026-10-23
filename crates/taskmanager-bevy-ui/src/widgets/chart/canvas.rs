//! The chart canvas: an antialiased raster of a continuous curve.
//!
//! The curve is drawn as ONE path per run — the stroke coverage is the
//! distance to the polyline itself, not a stack of per-segment quads. A
//! direction change therefore keeps a round join instead of opening a seam,
//! and every edge is antialiased by construction.
//!
//! The raster is plain data: the painter turns it into a texture, the
//! headless tests read it directly.

use crate::widgets::chart::geometry::ChartRun;
use bevy::color::Color;
use bevy::math::Vec2;

/// The alpha the wash under a curve starts at: the line's own ink at 35%,
/// fading to nothing at the floor — the reference surface's gradient.
pub(crate) const CANVAS_FILL_ALPHA: f32 = 0.35;

/// The vertical wash under the curve: the line's own ink, fading from
/// `alpha` at the area's top edge to nothing at the floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CanvasFill {
    pub(crate) color: Color,
    pub(crate) alpha: f32,
}

/// The grid the curve sits on: faint divisions plus the emphasized quarter
/// rules the reference surface draws under the curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CanvasGrid {
    pub(crate) color: Color,
    pub(crate) alpha: f32,
    /// Horizontal divisions (the reference surface uses five).
    pub(crate) horizontal: usize,
    /// Vertical divisions (the reference surface uses six).
    pub(crate) vertical: usize,
    /// Emphasized horizontal rules, as fractions of the height measured from
    /// the floor (quarter, half, three-quarter).
    pub(crate) reference: &'static [f32],
    pub(crate) reference_alpha: f32,
}

/// The pointer's vertical guide across the chart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CanvasCrosshair {
    pub(crate) x: f32,
    pub(crate) color: Color,
    pub(crate) alpha: f32,
}

/// The travelling highlight: a bright band that runs along the curve (the
/// charging flow a battery curve wears while it charges).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CanvasFlow {
    pub(crate) color: Color,
    /// The band's position along the curve, as a fraction of the width.
    pub(crate) phase: f32,
    /// The band's half-width, in px.
    pub(crate) radius: f32,
}

/// Everything one raster needs.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CanvasSpec {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) runs: Vec<ChartRun>,
    pub(crate) thickness: f32,
    pub(crate) line: Color,
    pub(crate) fill: Option<CanvasFill>,
    pub(crate) grid: Option<CanvasGrid>,
    pub(crate) crosshair: Option<CanvasCrosshair>,
    pub(crate) flow: Option<CanvasFlow>,
}

/// One rasterized canvas: straight-alpha sRGB pixels, row-major.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CanvasImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

/// Composite one straight-alpha source over a destination pixel.
fn over(destination: &mut [u8], source: [f32; 4]) {
    let alpha = source[3];
    if alpha <= 0.0 {
        return;
    }
    let destination_alpha = destination[3] as f32 / 255.0;
    let out_alpha = alpha + destination_alpha * (1.0 - alpha);
    if out_alpha <= 0.0 {
        destination[0] = 0;
        destination[1] = 0;
        destination[2] = 0;
        destination[3] = 0;
        return;
    }
    for channel in 0..3 {
        let behind = destination[channel] as f32 / 255.0;
        let front = source[channel];
        let mixed = (front * alpha + behind * destination_alpha * (1.0 - alpha)) / out_alpha;
        destination[channel] = (mixed.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    destination[3] = (out_alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
}

/// The distance from a point to a segment's spine.
fn distance_to_segment(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let projection = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * projection)
}

/// The curve's ordinate at one column, interpolated inside its run.
fn ordinate_at(run: &ChartRun, x: f32) -> Option<f32> {
    let vertices = &run.vertices;
    if vertices.is_empty() {
        return None;
    }
    if x <= vertices[0].x {
        return Some(vertices[0].y);
    }
    let last = vertices[vertices.len() - 1];
    if x >= last.x {
        return Some(last.y);
    }
    for pair in vertices.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if x >= start.x && x <= end.x {
            let span = end.x - start.x;
            if span <= f32::EPSILON {
                return Some(start.y);
            }
            let ratio = (x - start.x) / span;
            return Some(start.y + (end.y - start.y) * ratio);
        }
    }
    Some(last.y)
}

/// The stroke coverage of the whole curve: one buffer, the maximum coverage
/// of every segment. Taking the maximum (instead of compositing each
/// segment) is what keeps a joint from double-darkening.
fn stroke_coverage(spec: &CanvasSpec) -> Vec<f32> {
    let width = spec.width as usize;
    let height = spec.height as usize;
    let mut coverage = vec![0.0_f32; width * height];
    let half = (spec.thickness / 2.0).max(0.5);
    for run in &spec.runs {
        for pair in run.vertices.windows(2) {
            let start = Vec2::new(pair[0].x, pair[0].y);
            let end = Vec2::new(pair[1].x, pair[1].y);
            let min_x = start.x.min(end.x) - half - 1.0;
            let max_x = start.x.max(end.x) + half + 1.0;
            let min_y = start.y.min(end.y) - half - 1.0;
            let max_y = start.y.max(end.y) + half + 1.0;
            let first_x = min_x.floor().max(0.0) as u32;
            let last_x = (max_x.ceil() as i64).clamp(0, width as i64) as u32;
            let first_y = min_y.floor().max(0.0) as u32;
            let last_y = (max_y.ceil() as i64).clamp(0, height as i64) as u32;
            for y in first_y..last_y {
                for x in first_x..last_x {
                    let center = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    let distance = distance_to_segment(center, start, end);
                    let value = (half + 0.5 - distance).clamp(0.0, 1.0);
                    let index = y as usize * width + x as usize;
                    if value > coverage[index] {
                        coverage[index] = value;
                    }
                }
            }
        }
    }
    coverage
}

/// Paint the vertical wash under every run.
fn paint_fill(pixels: &mut [u8], spec: &CanvasSpec) {
    let Some(fill) = spec.fill else {
        return;
    };
    let width = spec.width as usize;
    let height = spec.height as usize;
    if width == 0 || height == 0 {
        return;
    }
    let ink = fill.color.to_srgba();
    let floor = (height - 1) as f32;
    for run in &spec.runs {
        let Some(top) = run
            .vertices
            .iter()
            .map(|vertex| vertex.y)
            .fold(None, |acc: Option<f32>, y| {
                Some(acc.map_or(y, |a| a.min(y)))
            })
        else {
            continue;
        };
        let first_x = run
            .vertices
            .first()
            .map(|vertex| vertex.x.floor().max(0.0) as u32)
            .unwrap_or(0);
        let last_x = run
            .vertices
            .last()
            .map(|vertex| (vertex.x.ceil() as i64).clamp(0, width as i64) as u32)
            .unwrap_or(0);
        for x in first_x..last_x {
            let Some(curve_y) = ordinate_at(run, x as f32 + 0.5) else {
                continue;
            };
            let start = (curve_y.ceil().max(0.0) as u32).min(height as u32);
            for y in start..height as u32 {
                let span = (floor - top).max(1.0);
                let decay = (1.0 - (y as f32 - top) / span).clamp(0.0, 1.0);
                let alpha = fill.alpha * decay;
                if alpha <= 0.0 {
                    continue;
                }
                let index = (y as usize * width + x as usize) * 4;
                over(
                    &mut pixels[index..index + 4],
                    [ink.red, ink.green, ink.blue, alpha],
                );
            }
        }
    }
}

/// Paint one dashed horizontal rule.
fn paint_dashed_rule(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    y: u32,
    ink: [f32; 3],
    alpha: f32,
    dash: usize,
) {
    if y >= height as u32 {
        return;
    }
    for x in 0..width {
        if (x / dash.max(1)) % 2 == 1 {
            continue;
        }
        let index = (y as usize * width + x) * 4;
        over(
            &mut pixels[index..index + 4],
            [ink[0], ink[1], ink[2], alpha],
        );
    }
}

/// Paint the grid divisions and the emphasized quarter rules.
fn paint_grid(pixels: &mut [u8], spec: &CanvasSpec) {
    let Some(grid) = spec.grid else {
        return;
    };
    let width = spec.width as usize;
    let height = spec.height as usize;
    if width == 0 || height == 0 {
        return;
    }
    let ink = grid.color.to_srgba();
    for index in 1..grid.horizontal.max(1) {
        let y = (spec.height as f32 * index as f32 / grid.horizontal.max(1) as f32).round() as u32;
        if y >= height as u32 {
            continue;
        }
        let row = (y as usize * width) * 4;
        for x in 0..width {
            let offset = row + x * 4;
            over(
                &mut pixels[offset..offset + 4],
                [ink.red, ink.green, ink.blue, grid.alpha],
            );
        }
    }
    for index in 1..grid.vertical.max(1) {
        let x = (spec.width as f32 * index as f32 / grid.vertical.max(1) as f32).round() as u32;
        if x >= width as u32 {
            continue;
        }
        for y in 0..height {
            let offset = (y * width + x as usize) * 4;
            over(
                &mut pixels[offset..offset + 4],
                [ink.red, ink.green, ink.blue, grid.alpha],
            );
        }
    }
    for fraction in grid.reference {
        let y = (spec.height as f32 * (1.0 - fraction)).round() as u32;
        paint_dashed_rule(
            pixels,
            width,
            height,
            y,
            [ink.red, ink.green, ink.blue],
            grid.reference_alpha,
            DASH_PERIOD_PX,
        );
    }
}

/// Paint the pointer's vertical guide.
fn paint_crosshair(pixels: &mut [u8], spec: &CanvasSpec) {
    let Some(crosshair) = spec.crosshair else {
        return;
    };
    let width = spec.width as usize;
    let height = spec.height as usize;
    if width == 0 || height == 0 {
        return;
    }
    let x = crosshair.x.round().clamp(0.0, width as f32 - 1.0) as u32;
    let ink = crosshair.color.to_srgba();
    for y in 0..height {
        if (y / DASH_PERIOD_PX.max(1)) % 2 == 1 {
            continue;
        }
        let offset = (y * width + x as usize) * 4;
        over(
            &mut pixels[offset..offset + 4],
            [ink.red, ink.green, ink.blue, crosshair.alpha],
        );
    }
}

/// The dash period of the emphasized rules and the pointer guide, in px.
const DASH_PERIOD_PX: usize = 4;

/// Rasterize one canvas: the wash first, then the grid, then the curve, then
/// the pointer guide — the same layer order the reference surface paints.
#[must_use]
pub(crate) fn rasterize(spec: &CanvasSpec) -> CanvasImage {
    let width = spec.width as usize;
    let height = spec.height as usize;
    let mut pixels = vec![0_u8; width * height * 4];
    if width == 0 || height == 0 {
        return CanvasImage {
            width: spec.width,
            height: spec.height,
            pixels,
        };
    }
    paint_fill(&mut pixels, spec);
    paint_grid(&mut pixels, spec);
    let coverage = stroke_coverage(spec);
    let ink = spec.line.to_srgba();
    for (index, value) in coverage.iter().enumerate() {
        if *value <= 0.0 {
            continue;
        }
        let offset = index * 4;
        over(
            &mut pixels[offset..offset + 4],
            [ink.red, ink.green, ink.blue, *value],
        );
    }
    paint_flow(&mut pixels, spec, &coverage);
    paint_crosshair(&mut pixels, spec);
    CanvasImage {
        width: spec.width,
        height: spec.height,
        pixels,
    }
}

/// Paint the travelling highlight along the curve.
fn paint_flow(pixels: &mut [u8], spec: &CanvasSpec, coverage: &[f32]) {
    let Some(flow) = spec.flow else {
        return;
    };
    let width = spec.width as usize;
    let height = spec.height as usize;
    if width == 0 || height == 0 {
        return;
    }
    let center = flow.phase.clamp(0.0, 1.0) * (width as f32 - 1.0);
    let radius = flow.radius.max(1.0);
    let ink = flow.color.to_srgba();
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let stroke = coverage[index];
            if stroke <= 0.0 {
                continue;
            }
            let distance = (x as f32 - center) / radius;
            let intensity = (-distance * distance).exp() * stroke;
            if intensity <= 0.0 {
                continue;
            }
            let offset = index * 4;
            over(
                &mut pixels[offset..offset + 4],
                [ink.red, ink.green, ink.blue, intensity],
            );
        }
    }
}
