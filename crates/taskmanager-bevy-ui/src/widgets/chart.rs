//! Bounded, gap-aware chart projection for the Bevy performance surface.
//!
//! Bevy UI 0.20 lays out the surface; this module keeps the measurement math
//! toolkit-neutral and makes the render seam a small `bsn!` scene. Non-finite
//! observations create gaps rather than joining across missing data.
//!
//! The curve itself is a continuous canvas: one raster per chart, stroked as
//! a single path, so a direction change keeps a round join instead of opening
//! a seam between two rotated rectangles.

pub(crate) mod canvas;
pub(crate) mod geometry;
pub(crate) mod hover;
pub(crate) mod painter;

pub(crate) use geometry::ChartSegment;
pub(crate) use geometry::{MAX_CHART_POINTS, line_segments};
pub(crate) use hover::{ChartHoverLabel, ChartPointer};
pub(crate) use painter::{
    ChartCanvas, ChartContent, ChartPaintState, ChartStyle, chart_canvas_scene,
};

use crate::scene_attach::insert_into;
use crate::widgets::chart::geometry::{points_extent, segment_vertices};
use crate::widgets::chart::painter::{ChartTextures, paint_chart_canvases};
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, SystemParam};
use bevy::math::Vec2;
use bevy::scene::Scene;

/// The layout/render anchor for a chart surface. The count is the number of
/// plotted points — metadata the surface publishes so the render path and
/// the tests read the same truth.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChartSurface(pub(crate) usize);

/// Last successfully painted logical chart size, shared by inspection surfaces.
#[derive(Component, Clone, Default)]
pub(crate) struct CurveMeasurement(pub(crate) Option<(f32, f32)>);

/// The polyline render adapter: one continuous canvas carrying the curve.
///
/// The segments are a derived view of the projection; the canvas strokes
/// their points as a single path, so the rendered curve has no per-segment
/// seams.
pub(crate) fn polyline_scene(segments: &[ChartSegment], color: Color) -> impl Scene + use<> {
    let vertices = segment_vertices(segments);
    let extent = points_extent(&vertices);
    chart_canvas_scene(
        ChartCanvas {
            content: ChartContent::Points {
                vertices,
                source: Vec2::new(extent.0, extent.1),
            },
            style: ChartStyle {
                line: color,
                ..ChartStyle::default()
            },
        },
        Vec::new(),
    )
}

/// The render seam's access: the host's children, the canvases they carry,
/// and the command queue.
#[derive(SystemParam)]
pub(crate) struct CurvePaintAccess<'w, 's> {
    pub(crate) children: Query<'w, 's, &'static Children>,
    pub(crate) canvases: Query<'w, 's, &'static mut painter::ChartCanvas>,
    commands: Commands<'w, 's>,
}

/// Publish a curve onto a surface through the owned scene adapter.
///
/// The surface owns the canvas content: the samples travel with the surface's
/// own component, so the painter measures the slot and projects against that
/// measurement instead of a fixed design width. Failed scene publication
/// cannot certify a rendered curve for capture readiness.
pub(crate) fn paint_curve_at_size(
    access: &mut CurvePaintAccess,
    entity: Entity,
    size: Vec2,
    samples: &[f32],
    ceiling: f32,
    color: Color,
) -> bool {
    if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
        return false;
    }
    let canvas = ChartCanvas {
        content: ChartContent::Samples {
            values: samples.to_vec(),
            ceiling,
        },
        style: ChartStyle {
            line: color,
            ..ChartStyle::default()
        },
    };
    insert_into(
        &mut access.commands,
        entity,
        (canvas, ChartPaintState::default()),
    );
    true
}

/// Register the chart painter: the canvas textures, the pointer seam, and
/// the per-frame raster.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<ChartTextures>();
    app.add_observer(hover::on_chart_pointer_move);
    app.add_observer(hover::on_chart_pointer_out);
    app.add_systems(
        bevy::app::PostUpdate,
        paint_chart_canvases.after(bevy::ui::UiSystems::PostLayout),
    );
}

#[cfg(test)]
#[path = "../../tests/headless/chart.rs"]
mod tests;
