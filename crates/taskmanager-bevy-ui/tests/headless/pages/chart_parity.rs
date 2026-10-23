//! test-intent: behavior
//!
//! The chart surface's geometry contract:
//!
//! - the curve is ONE continuous antialiased path — a direction change keeps
//!   a round join instead of opening the seam a stack of per-segment
//!   rectangles leaves;
//! - the wash under the curve fades from the line's own ink to nothing;
//! - a fresh sample slides in one slot instead of jumping;
//! - the pointer resolves the sample it is really over;
//! - the grid carries its quarter rules and the scale ruler names the scale;
//! - the curve projects onto the slot the layout measured — never onto a
//!   fixed design width.

use super::*;

use crate::widgets::chart::canvas::{
    CANVAS_FILL_ALPHA, CanvasFill, CanvasGrid, CanvasSpec, rasterize,
};
use crate::widgets::chart::geometry::{
    ChartRun, ChartScale, ChartVertex, chart_runs, line_segments,
};
use crate::widgets::chart::hover::{hover_position, hover_slot, hover_text};
use crate::widgets::chart::painter::slide_offset;
use bevy::color::Color;
use bevy::math::Vec2;

/// The coverage a per-segment rectangle would paint at one pixel: the old
/// pseudo-chart drew one rotated rectangle per segment, with butt caps, so
/// the wedge outside a direction change stayed empty.
fn rectangle_coverage(start: ChartVertex, end: ChartVertex, thickness: f32, x: u32, y: u32) -> f32 {
    let center = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
    let from = Vec2::new(start.x, start.y);
    let to = Vec2::new(end.x, end.y);
    let segment = to - from;
    let length_squared = segment.length_squared();
    if length_squared <= f32::EPSILON {
        return 0.0;
    }
    let projection = ((center - from).dot(segment) / length_squared).clamp(0.0, 1.0);
    if projection <= 0.0 || projection >= 1.0 {
        // A butt cap: the rectangle stops at the segment's end.
        return 0.0;
    }
    let distance = center.distance(from + segment * projection);
    (thickness / 2.0 + 0.5 - distance).clamp(0.0, 1.0)
}

/// The coverage of one pixel of a raster, in `0.0..=1.0`.
fn alpha_at(image: &crate::widgets::chart::canvas::CanvasImage, x: u32, y: u32) -> f32 {
    if x >= image.width || y >= image.height {
        return 0.0;
    }
    let index = ((y * image.width + x) * 4 + 3) as usize;
    image.pixels.get(index).copied().unwrap_or(0) as f32 / 255.0
}

fn spec(width: u32, height: u32, runs: Vec<ChartRun>, thickness: f32) -> CanvasSpec {
    CanvasSpec {
        width,
        height,
        runs,
        thickness,
        line: Color::WHITE,
        fill: None,
        grid: None,
        crosshair: None,
        flow: None,
    }
}

#[test]
fn the_stroke_joins_at_the_corner_where_per_segment_rectangles_leave_a_notch() {
    // A V-shaped curve: the vertex at (30, 30) opens upward, so the wedge
    // BELOW the vertex belongs to no segment. A per-segment rectangle
    // rendering leaves that wedge empty (the seam the pseudo-chart opened);
    // one continuous path fills it with a round join.
    let vertices = vec![
        ChartVertex { x: 10.0, y: 10.0 },
        ChartVertex { x: 30.0, y: 30.0 },
        ChartVertex { x: 50.0, y: 10.0 },
    ];
    let runs = vec![ChartRun {
        vertices: vertices.clone(),
    }];
    let canvas = rasterize(&spec(60, 40, runs, 4.0));

    assert!(
        alpha_at(&canvas, 30, 31) > 0.5,
        "the continuous stroke fills the wedge below the corner (alpha {})",
        alpha_at(&canvas, 30, 31)
    );
    let reference = rectangle_coverage(vertices[0], vertices[1], 4.0, 30, 31)
        + rectangle_coverage(vertices[1], vertices[2], 4.0, 30, 31);
    assert!(
        reference < 0.01,
        "the per-segment rectangles really do leave that wedge empty (coverage {reference})"
    );
}

#[test]
fn the_curve_is_one_path_over_its_whole_window() {
    // A window without gaps is ONE run: the renderer strokes a single path,
    // so a direction change can never open a seam. The segment list stays a
    // derived view of that path.
    let samples = [1.0, 3.0, 2.0, 5.0];
    let runs = chart_runs(&samples, 100.0, 40.0, 64, ChartScale::Window);
    assert_eq!(runs.len(), 1, "a gap-free window is one continuous path");
    assert_eq!(runs[0].vertices.len(), samples.len());
    assert_eq!(
        line_segments(&samples, 100.0, 40.0, 64).len(),
        samples.len() - 1,
        "the segment list is derived from the path, not the drawing truth"
    );

    // A missing observation breaks the path instead of joining across it: the
    // lone sample before the gap cannot form a path of its own, so the
    // measured stretch after the gap is the one continuous path.
    let gapped = chart_runs(
        &[1.0, f32::NAN, 3.0, 4.0],
        100.0,
        40.0,
        64,
        ChartScale::Window,
    );
    assert_eq!(gapped.len(), 1, "the gap opens the next path");
    assert_eq!(gapped[0].vertices.len(), 2, "the gap joins nothing");
    assert!(
        (gapped[0].vertices[0].x - 100.0 * 2.0 / 3.0).abs() < 1e-3,
        "the path after the gap keeps its own slot (x {})",
        gapped[0].vertices[0].x
    );
}

#[test]
fn the_stroke_edge_is_antialiased() {
    // A diagonal curve's edge pixel carries a partial coverage: the curve is
    // a shaded path, not a hard-edged block.
    let runs = vec![ChartRun {
        vertices: vec![
            ChartVertex { x: 5.0, y: 5.0 },
            ChartVertex { x: 55.0, y: 35.0 },
        ],
    }];
    let canvas = rasterize(&spec(60, 40, runs, 3.0));
    let mut partial = 0;
    for y in 0..40 {
        for x in 0..60 {
            let alpha = alpha_at(&canvas, x, y);
            if alpha > 0.05 && alpha < 0.95 {
                partial += 1;
            }
        }
    }
    assert!(
        partial > 8,
        "the curve's edge carries antialiased pixels (partial pixels: {partial})"
    );
}

#[test]
fn the_wash_under_the_curve_fades_to_nothing() {
    // The area fill starts at the line's own ink (35% alpha) at the curve
    // and decays to nothing at the floor — the reference surface's gradient.
    let runs = vec![ChartRun {
        vertices: vec![
            ChartVertex { x: 0.0, y: 10.0 },
            ChartVertex { x: 60.0, y: 10.0 },
        ],
    }];
    let mut fill = spec(60, 40, runs, 2.0);
    fill.fill = Some(CanvasFill {
        color: Color::WHITE,
        alpha: CANVAS_FILL_ALPHA,
    });
    let canvas = rasterize(&fill);

    let top = alpha_at(&canvas, 5, 11);
    let middle = alpha_at(&canvas, 5, 25);
    let floor = alpha_at(&canvas, 5, 39);
    assert!(
        top > 0.25,
        "the wash starts at the curve's own ink (alpha {top})"
    );
    assert!(
        middle < top && middle > 0.0,
        "the wash decays down the area (alpha {middle} vs {top})"
    );
    assert!(
        floor < 0.02,
        "the wash reaches nothing at the floor (alpha {floor})"
    );
}

#[test]
fn a_fresh_sample_slides_in_one_slot_instead_of_jumping() {
    // The settled curve owns the full width; the fresh sample enters from
    // the right, one slot away, and slides left into place.
    let width = 100.0;
    let points = 11;
    let slot = width / (points - 1) as f32;
    assert!((slide_offset(0.0, width, points) + slot).abs() < 1e-4);
    assert!(slide_offset(1.0, width, points).abs() < 1e-4);
    let middle = slide_offset(0.5, width, points);
    assert!(
        middle > slide_offset(0.0, width, points) && middle < slide_offset(1.0, width, points),
        "the slide eases instead of stepping (offset {middle})"
    );
}

#[test]
fn the_pointer_resolves_the_sample_it_is_over() {
    // The newest sample owns the right edge, so the slot is resolved from
    // the right: a sparse window can never report a value from the far side.
    assert_eq!(hover_slot(10, 100.0, 100.0), Some(9));
    assert_eq!(hover_slot(10, 100.0, 0.0), Some(0));
    assert_eq!(hover_slot(10, 100.0, 95.0), Some(9));
    assert_eq!(hover_slot(10, 100.0, 4.0), Some(0));
    assert_eq!(hover_slot(0, 100.0, 10.0), None);

    // The picker's normalized hit maps onto the canvas's own box.
    let center = hover_position(Some(Vec2::new(0.0, 0.0)), Vec2::new(100.0, 40.0));
    assert_eq!(center, Some(Vec2::new(50.0, 20.0)));
    assert_eq!(hover_position(None, Vec2::new(100.0, 40.0)), None);

    // The pill reads the hovered value and how far the slot sits behind the
    // newest sample.
    assert_eq!(hover_text(Some(percent_readout), 42.0, 0), "42%");
    assert_eq!(hover_text(Some(percent_readout), 42.0, 3), "42% · -3");
}

#[test]
fn the_grid_carries_its_quarter_rules() {
    // The faint divisions and the emphasized quarter rules live under the
    // curve; the rows between them stay empty.
    let runs = vec![ChartRun {
        vertices: vec![
            ChartVertex { x: 0.0, y: 40.0 },
            ChartVertex { x: 100.0, y: 40.0 },
        ],
    }];
    let mut grid = spec(100, 80, runs, 2.0);
    grid.grid = Some(CanvasGrid {
        color: Color::WHITE,
        alpha: 0.13,
        horizontal: 5,
        vertical: 6,
        reference: &[0.25, 0.5, 0.75],
        reference_alpha: 0.26,
    });
    let canvas = rasterize(&grid);

    // The quarter rules sit at 25%, 50% and 75% of the height, measured
    // from the floor: rows 60, 40 and 20 of an 80px canvas.
    // The column sits between the vertical divisions, so only the horizontal
    // rules can paint it.
    for row in [20_u32, 40, 60] {
        assert!(
            alpha_at(&canvas, 25, row) > 0.0,
            "the reference rule paints row {row}"
        );
    }
    assert!(
        alpha_at(&canvas, 25, 28) < 0.02,
        "the rows between the rules stay empty"
    );
}

#[test]
fn the_scale_ruler_names_the_curve_own_scale() {
    // The ruler reads the curve's own scale: percentages for the utilization
    // curves, the measured rate for the throughput curve.
    let percent = ruler_labels(SystemCurve::Cpu, 100.0);
    assert_eq!(percent.first().map(String::as_str), Some("100%"));
    assert_eq!(percent.get(1).map(String::as_str), Some("75%"));
    assert_eq!(percent.last().map(String::as_str), Some("0%"));

    let rate = ruler_labels(SystemCurve::Network, 2_048.0);
    assert_eq!(rate.first().map(String::as_str), Some("2.0 KiB/s"));
    assert_eq!(rate.last().map(String::as_str), Some("0 B/s"));
}

#[test]
fn the_curve_projects_onto_the_slot_the_layout_measured() {
    // The same window projected into two different slots fills both of them:
    // the width is the measurement, never a design constant.
    let samples = [10.0, 20.0, 30.0];
    let narrow = chart_runs(&samples, 200.0, 40.0, 64, ChartScale::Ceiling(100.0));
    let wide = chart_runs(&samples, 400.0, 40.0, 64, ChartScale::Ceiling(100.0));
    assert!((narrow[0].vertices.last().map(|v| v.x).unwrap_or(0.0) - 200.0).abs() < 1e-4);
    assert!((wide[0].vertices.last().map(|v| v.x).unwrap_or(0.0) - 400.0).abs() < 1e-4);
    assert!(
        wide[0].vertices[1].x > narrow[0].vertices[1].x,
        "the wider slot spreads the samples further apart"
    );
}

#[test]
fn the_npu_card_carries_its_own_realtime_curve() {
    // The NPU card is one of the strip's curves, and its window is the NPU
    // usage history: two observed inventories make a real trend, and an
    // unobserved accelerator makes no curve at all.
    use crate::pages::performance::curve_samples;
    use taskmanager_core::core::DeviceGeneration;
    use taskmanager_core::core::metrics::ScalarObservation;
    use taskmanager_core::core::npu::{NpuDevice, NpuInventorySnapshot};
    use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

    assert!(
        SystemCurve::STRIP.contains(&SystemCurve::Npu),
        "the NPU card is one of the strip's curves"
    );

    let mut shell = ShellApp::new();
    assert!(
        curve_samples(&shell, SystemCurve::Npu).is_empty(),
        "an unobserved accelerator carries no curve"
    );

    for utilization in [20.0_f32, 55.0] {
        let mut device = NpuDevice {
            device_id: "npu0".into(),
            device_generation: DeviceGeneration::new(1),
            brand: None,
            driver: None,
            utilization_pct: ScalarObservation::available(utilization, 1_000),
            engines: Vec::new(),
            memory: Default::default(),
        };
        device.device_generation = DeviceGeneration::new(1);
        let snapshot = NpuInventorySnapshot::discovered(vec![device], 1_000);
        seed_projection_fact(&mut shell, ProjectionSeedFact::NpuInventory(Some(snapshot)));
    }

    let samples = curve_samples(&shell, SystemCurve::Npu);
    assert_eq!(samples.len(), 2, "the NPU card carries its own window");
    assert!(
        samples.iter().all(|sample| sample.is_finite()),
        "the NPU window is a real trend, never a fabricated flat line"
    );
}
