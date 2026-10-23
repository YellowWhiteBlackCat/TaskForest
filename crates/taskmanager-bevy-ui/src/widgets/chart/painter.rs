//! The chart render adapter: the canvas node, its raster, and the painter
//! that keeps the raster in step with the measured slot.
//!
//! The scene declares the canvas as a node plus a [`ChartCanvas`] marker; the
//! painter measures that node, projects the content onto the measurement and
//! writes the raster into the node's texture. Nothing about the curve's
//! geometry is fixed: the width is always the slot the layout gave the
//! chart, so the curve fills the card whatever the window does.

use std::collections::HashMap;

use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::ecs::world::EntityWorldMut;
use bevy::image::Image;
use bevy::math::Vec2;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::scene::{EntityWorldMutSceneExt, Scene, bsn};
use bevy::time::Time;
use bevy::ui::widget::{ImageNode, NodeImageMode, Text};
use bevy::ui::{ComputedNode, Display, Node, Overflow, PositionType, Val, percent, px};

use crate::widgets::chart::canvas::{
    CanvasCrosshair, CanvasFill, CanvasFlow, CanvasGrid, CanvasImage, CanvasSpec, rasterize,
};
use crate::widgets::chart::geometry::{
    CHART_LINE_THICKNESS_PX, ChartRun, ChartScale, ChartVertex, MAX_CHART_POINTS, chart_runs,
};
use crate::widgets::chart::hover::{
    ChartHoverLabel, ChartPointer, hover_position, hover_slot, hover_text,
};

/// How long one sample slot takes to slide in, in seconds — the same slide
/// the reference surface runs.
pub(crate) const CHART_SLIDE_SECONDS: f32 = 0.18;

/// How long the travelling highlight takes to run the curve, in seconds.
pub(crate) const CHART_FLOW_SECONDS: f32 = 1.6;

/// The smallest slot a canvas paints into; below it there is nothing to see.
const MIN_CANVAS_EDGE_PX: f32 = 1.0;

/// How far the value pill sits from the crosshair, in px.
const HOVER_PILL_OFFSET_PX: f32 = 6.0;

/// What a canvas draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum ChartContent {
    /// Points already projected into `source`; the painter re-projects them
    /// onto the slot it measured.
    Points {
        vertices: Vec<ChartVertex>,
        source: Vec2,
    },
    /// Raw samples; the painter projects them against the measured slot.
    Samples { values: Vec<f32>, ceiling: f32 },
    /// Nothing measured yet.
    #[default]
    Empty,
}

/// How a canvas draws its content.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChartStyle {
    /// The curve's ink.
    pub(crate) line: Color,
    /// The stroke's thickness in px.
    pub(crate) thickness: f32,
    /// The vertical wash under the curve.
    pub(crate) fill: Option<CanvasFill>,
    /// The grid the curve sits on.
    pub(crate) grid: Option<CanvasGrid>,
    /// The travelling highlight along the curve (a charging flow).
    pub(crate) flow: Option<CanvasFlow>,
    /// Formats the hovered value for the floating pill.
    pub(crate) format: Option<fn(f32) -> String>,
}

impl Default for ChartStyle {
    fn default() -> Self {
        Self {
            line: Color::WHITE,
            thickness: CHART_LINE_THICKNESS_PX,
            fill: None,
            grid: None,
            flow: None,
            format: None,
        }
    }
}

/// The canvas marker: the chart's content and style, declared by the scene.
#[derive(Component, Clone, Debug, Default)]
pub(crate) struct ChartCanvas {
    pub(crate) content: ChartContent,
    pub(crate) style: ChartStyle,
}

/// The painter's per-canvas state: the slide progress and the signature of
/// the raster currently on the texture.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ChartPaintState {
    /// The signature of the content the current slide was started for.
    pub(crate) content_hash: u64,
    /// The slide progress: 0 = the previous window, 1 = settled.
    pub(crate) slide: f32,
    /// The signature of the raster on the texture.
    pub(crate) signature: u64,
    /// Whether a raster was ever written for this canvas.
    pub(crate) painted: bool,
    /// The travelling highlight's position along the curve.
    pub(crate) flow_phase: f32,
}

/// The canvas textures, keyed by the canvas entity.
#[derive(Resource, Default)]
pub(crate) struct ChartTextures {
    handles: HashMap<Entity, Handle<Image>>,
    sizes: HashMap<Entity, (u32, u32)>,
    stamped: HashMap<Entity, Handle<Image>>,
}

impl ChartTextures {
    /// The texture a canvas draws into, created (or resized) on demand.
    fn texture(
        &mut self,
        entity: Entity,
        images: &mut Assets<Image>,
        width: u32,
        height: u32,
    ) -> Option<Handle<Image>> {
        if self.sizes.get(&entity) == Some(&(width, height))
            && let Some(handle) = self.handles.get(&entity)
        {
            return Some(handle.clone());
        }
        let image = Image::new_fill(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        );
        let handle = images.add(image);
        self.handles.insert(entity, handle.clone());
        self.sizes.insert(entity, (width, height));
        Some(handle)
    }

    /// Whether the canvas node already carries this texture.
    fn stamped_with(&self, entity: Entity, handle: &Handle<Image>) -> bool {
        self.stamped.get(&entity) == Some(handle)
    }

    /// Remember the texture the canvas node carries.
    fn stamp(&mut self, entity: Entity, handle: &Handle<Image>) {
        self.stamped.insert(entity, handle.clone());
    }

    /// Forget the canvases that are no longer mounted.
    fn retain(&mut self, live: &[Entity]) {
        self.handles.retain(|entity, _| live.contains(entity));
        self.sizes.retain(|entity, _| live.contains(entity));
        self.stamped.retain(|entity, _| live.contains(entity));
    }
}

/// The projection of a canvas's content onto the measured slot.
fn canvas_runs(canvas: &ChartCanvas, size: Vec2) -> Vec<ChartRun> {
    match &canvas.content {
        ChartContent::Samples { values, ceiling } => chart_runs(
            values,
            size.x,
            size.y,
            MAX_CHART_POINTS,
            ChartScale::Ceiling(*ceiling),
        ),
        ChartContent::Points { vertices, source } => {
            if vertices.is_empty() || source.x <= 0.0 || source.y <= 0.0 {
                return Vec::new();
            }
            let scale = Vec2::new(size.x / source.x, size.y / source.y);
            vec![ChartRun {
                vertices: vertices
                    .iter()
                    .map(|vertex| ChartVertex {
                        x: vertex.x * scale.x,
                        y: vertex.y * scale.y,
                    })
                    .collect(),
            }]
        }
        ChartContent::Empty => Vec::new(),
    }
}

/// The horizontal shift of the settling slide: the curve enters one slot from
/// the right and slides left into place.
pub(crate) fn slide_offset(progress: f32, width: f32, points: usize) -> f32 {
    if points < 2 {
        return 0.0;
    }
    let eased = progress.clamp(0.0, 1.0);
    let eased = eased * eased * (3.0 - 2.0 * eased);
    let slot = width / (points - 1) as f32;
    -(1.0 - eased) * slot
}

/// Mix one value into a content signature.
fn mix(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(0x0100_0000_01b3);
}

/// The signature of a canvas's content: the same bytes always describe the
/// same curve, so an unchanged window costs no raster.
fn content_signature(content: &ChartContent) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    match content {
        ChartContent::Samples { values, ceiling } => {
            mix(&mut hash, u64::from(ceiling.to_bits()));
            for value in values {
                mix(&mut hash, u64::from(value.to_bits()));
            }
        }
        ChartContent::Points { vertices, source } => {
            mix(&mut hash, u64::from(source.x.to_bits()));
            mix(&mut hash, u64::from(source.y.to_bits()));
            for vertex in vertices {
                mix(&mut hash, u64::from(vertex.x.to_bits()));
                mix(&mut hash, u64::from(vertex.y.to_bits()));
            }
        }
        ChartContent::Empty => {}
    }
    hash
}

/// The scene of one canvas: a clipped surface the painter fills, with the
/// page's overlays (the value pill) riding on top of it.
pub(crate) fn chart_canvas_scene(
    canvas: ChartCanvas,
    overlays: Vec<Box<dyn Scene>>,
) -> impl Scene + use<> {
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            overflow: Overflow::clip(),
        }
        ChartCanvas { content: {canvas.content}, style: {canvas.style} }
        ChartPointer
        ChartPaintState
        Children [ { overlays } ]
    }
}

/// The painter's query surface.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ChartPaintAccess<'w, 's> {
    canvases: Query<
        'w,
        's,
        (
            Entity,
            &'static ChartCanvas,
            &'static mut ChartPaintState,
            &'static ComputedNode,
            Option<&'static ChartPointer>,
        ),
    >,
    labels: Query<
        'w,
        's,
        (
            Entity,
            &'static ChartHoverLabel,
            &'static ChildOf,
            &'static mut Node,
            Option<&'static ComputedNode>,
        ),
    >,
    texts: Query<'w, 's, &'static mut Text>,
    children: Query<'w, 's, &'static Children>,
}

/// Paint every mounted canvas: measure the slot, project the content onto
/// it, rasterize and write the texture.
pub(crate) fn paint_chart_canvases(
    mut access: ChartPaintAccess,
    mut images: Option<ResMut<Assets<Image>>>,
    mut textures: ResMut<ChartTextures>,
    mut commands: Commands,
    time: Res<Time>,
) {
    // A composition without an image store has no surface to paint into:
    // the chart stays an honest absence, never a crash.
    let Some(mut images) = images.take() else {
        return;
    };
    let delta = time.delta_secs();
    let mut live: Vec<Entity> = Vec::new();
    for (entity, canvas, mut state, node, pointer) in &mut access.canvases {
        live.push(entity);
        let size = node.size() * node.inverse_scale_factor();
        if size.x < MIN_CANVAS_EDGE_PX || size.y < MIN_CANVAS_EDGE_PX {
            continue;
        }
        let mut runs = canvas_runs(canvas, size);
        let points: usize = runs.iter().map(|run| run.vertices.len()).sum();
        // A fresh window restarts the slide: the curve enters one slot from
        // the right instead of jumping. The CONTENT decides — a full window
        // shifts by one sample without changing its length.
        let content_hash = content_signature(&canvas.content);
        if state.content_hash != content_hash {
            state.content_hash = content_hash;
            state.slide = 0.0;
        }
        if state.slide < 1.0 {
            state.slide = (state.slide + delta / CHART_SLIDE_SECONDS).min(1.0);
        }
        if canvas.style.flow.is_some() {
            state.flow_phase = (state.flow_phase + delta / CHART_FLOW_SECONDS) % 1.0;
        }
        let offset = slide_offset(state.slide, size.x, points);
        for run in &mut runs {
            for vertex in &mut run.vertices {
                vertex.x += offset;
            }
        }
        let hover = hover_position(pointer.and_then(|pointer| pointer.normalized), size);
        let slot = hover.and_then(|position| hover_slot(points, size.x, position.x));
        let signature = canvas_signature(canvas, size, state.slide, state.flow_phase, hover);
        if state.painted && state.signature == signature {
            continue;
        }
        state.signature = signature;
        state.painted = true;
        let spec = CanvasSpec {
            width: size.x.round().max(1.0) as u32,
            height: size.y.round().max(1.0) as u32,
            runs,
            thickness: canvas.style.thickness,
            line: canvas.style.line,
            fill: canvas.style.fill,
            grid: canvas.style.grid,
            crosshair: hover.map(|position| CanvasCrosshair {
                x: position.x,
                color: canvas.style.line,
                alpha: 0.55,
            }),
            flow: canvas.style.flow.map(|flow| CanvasFlow {
                color: flow.color,
                phase: state.flow_phase,
                radius: flow.radius,
            }),
        };
        let image = rasterize(&spec);
        let Some(handle) = textures.texture(entity, &mut images, image.width, image.height) else {
            continue;
        };
        write_pixels(&mut images, &handle, &image);
        if !textures.stamped_with(entity, &handle) {
            textures.stamp(entity, &handle);
            stamp_canvas_texture(&mut commands, entity, handle);
        }
        update_hover_pill(
            entity,
            HoverReadout {
                canvas,
                slot,
                x: hover.map(|position| position.x),
                points,
                width: size.x,
            },
            &mut access.labels,
            &mut access.texts,
            &access.children,
        );
    }
    textures.retain(&live);
}

/// The signature of everything the raster depends on.
fn canvas_signature(
    canvas: &ChartCanvas,
    size: Vec2,
    slide: f32,
    flow_phase: f32,
    hover: Option<Vec2>,
) -> u64 {
    let mut hash = content_signature(&canvas.content);
    mix(&mut hash, u64::from(size.x.to_bits()));
    mix(&mut hash, u64::from(size.y.to_bits()));
    mix(&mut hash, u64::from(slide.to_bits()));
    mix(&mut hash, u64::from(flow_phase.to_bits()));
    if let Some(position) = hover {
        mix(&mut hash, u64::from(position.x.to_bits()));
        mix(&mut hash, u64::from(position.y.to_bits()));
    }
    hash
}

/// Write the raster into the canvas texture.
fn write_pixels(images: &mut Assets<Image>, handle: &Handle<Image>, image: &CanvasImage) {
    let Some(mut asset) = images.get_mut(handle) else {
        return;
    };
    if asset.data.as_ref().map(Vec::len) != Some(image.pixels.len()) {
        asset.data = Some(image.pixels.clone());
        return;
    }
    if let Some(data) = asset.data.as_mut() {
        data.copy_from_slice(&image.pixels);
    }
}

/// Point the canvas node at its texture.
///
/// The patch runs through a SILENCED queue: a surface that rebuilds in the
/// same frame can despawn the canvas before the command lands, and the raw
/// path would turn that race into a production panic.
fn stamp_canvas_texture(commands: &mut Commands, entity: Entity, handle: Handle<Image>) {
    commands
        .entity(entity)
        .queue_silenced(move |mut entity: EntityWorldMut| {
            let _ = entity.apply_scene(bsn! {
                ImageNode { image: {handle}, image_mode: NodeImageMode::Stretch }
            });
        });
}

/// The value under the crosshair, when the canvas holds samples.
fn hovered_value(canvas: &ChartCanvas, slot: usize) -> Option<f32> {
    match &canvas.content {
        ChartContent::Samples { values, .. } => values.get(slot).copied().filter(|v| v.is_finite()),
        _ => None,
    }
}

/// The hover's resolved state for one canvas: the slot under the pointer, the
/// pointer's own x, and the canvas the readout belongs to.
struct HoverReadout<'a> {
    canvas: &'a ChartCanvas,
    slot: Option<usize>,
    x: Option<f32>,
    points: usize,
    width: f32,
}

/// Update the floating pill: the hovered value, or no pill at all.
fn update_hover_pill(
    canvas_entity: Entity,
    hover: HoverReadout<'_>,
    labels: &mut Query<(
        Entity,
        &ChartHoverLabel,
        &ChildOf,
        &mut Node,
        Option<&ComputedNode>,
    )>,
    texts: &mut Query<&mut Text>,
    children: &Query<&Children>,
) {
    for (label_entity, _, parent, mut node, measured) in labels.iter_mut() {
        if parent.parent() != canvas_entity {
            continue;
        }
        let readout = hover.slot.and_then(|slot| {
            let value = hovered_value(hover.canvas, slot)?;
            let behind = hover.points.saturating_sub(1).saturating_sub(slot);
            Some(hover_text(hover.canvas.style.format, value, behind))
        });
        match readout {
            Some(text) => {
                node.display = Display::Flex;
                if let Some(x) = hover.x {
                    node.left = Val::Px(pill_left(
                        x,
                        hover.width,
                        measured.map(|node| node.size().x),
                    ));
                }
                if let Ok(children) = children.get(label_entity)
                    && let Some(text_entity) = children.first()
                    && let Ok(mut label) = texts.get_mut(*text_entity)
                {
                    label.0 = text;
                }
            }
            None => {
                node.display = Display::None;
            }
        }
    }
}

/// The pill's left edge: beside the crosshair, but never outside the canvas.
fn pill_left(crosshair_x: f32, canvas_width: f32, pill_width: Option<f32>) -> f32 {
    let width = pill_width.unwrap_or(0.0);
    let right = crosshair_x + HOVER_PILL_OFFSET_PX;
    if right + width > canvas_width {
        // The pill would leave the canvas on the right: it rides the left of
        // the crosshair instead.
        (crosshair_x - HOVER_PILL_OFFSET_PX - width).max(0.0)
    } else {
        right.max(0.0)
    }
}
