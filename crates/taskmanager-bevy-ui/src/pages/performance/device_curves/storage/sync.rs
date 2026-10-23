//! The storage surface's systems: the mount, the in-place refresh, the
//! expand/collapse motion, and the end-of-frame retirement.
//!
//! The mount replaces a bounded subtree of the disk's own card; the refresh
//! writes the folded numbers into the mounted rings and gauge; the motion
//! carries the typed expand/collapse state. A surface that is replaced is
//! hidden now and despawned at the end of the frame, so no observer ever
//! meets a vanished child.

use super::{
    ExpandTarget, GAUGE_START, GAUGE_SWEEP, RING_DESIGN, RING_START, StorageDonutArc,
    StorageDonutText, StorageExpand, StorageGaugeArc, StorageGaugeText, StorageRetired,
    StorageSurface, advance_expand, arc_vertices, band_ink, gauge_readout, partition_fraction,
    partition_readout, share_ink, storage_signature, storage_surface_scene,
};

use bevy::ecs::entity::Entity;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use bevy::math::Vec2;
use bevy::prelude::Visibility;
use bevy::scene::CommandsSceneExt;
use bevy::ui::widget::Text;
use bevy::ui::{Node, Val};

use crate::app::ShellTrack;
use crate::pages::performance::metrics::nvme_gauge_model;
use crate::pages::performance::{DynBlock, Section};
use crate::scene_attach::{attach_child, insert_into};
use crate::widgets::chart::{ChartCanvas, ChartContent};
use crate::window::WindowPalette;
use taskmanager_core::core::metrics::DiskMetrics;

/// The systems that own the storage surface.
#[derive(SystemParam)]
struct StorageAccess<'w, 's> {
    palette: Res<'w, WindowPalette>,
    blocks: Query<'w, 's, (Entity, &'static DynBlock)>,
    surfaces: Query<'w, 's, (Entity, &'static StorageSurface, &'static mut StorageExpand)>,
    retired: ResMut<'w, StorageRetired>,
    commands: Commands<'w, 's>,
}

/// Reconcile the mounted storage surfaces with the folded projection.
///
/// A disk that arrived gets a surface that slides open; a disk whose
/// structural facts changed gets a replacement that keeps the motion's
/// progress; a disk that left gets a surface that slides shut.
fn sync_storage_surfaces(track: ShellTrack, mut access: StorageAccess) {
    let shell = track.shell();
    let disks: Vec<DiskMetrics> = crate::pages::performance::metrics::disks(shell)
        .map(|disks| disks.to_vec())
        .unwrap_or_default();
    let filesystems = shell
        .projection()
        .storage_health_projection()
        .map(|(snapshot, _)| snapshot);
    for disk in &disks {
        let Some(block) = access
            .blocks
            .iter()
            .find(|(_, block)| block.0 == Section::Disk && block.1 == disk.device_id)
            .map(|(entity, _)| entity)
        else {
            continue;
        };
        let signature = storage_signature(disk, filesystems);
        let mounted = access
            .surfaces
            .iter()
            .find(|(_, surface, _)| surface.disk == disk.device_id);
        match mounted {
            Some((_, surface, _)) if surface.signature == signature => {}
            Some((entity, _, expand)) => {
                // The structure changed: the old surface stops drawing now and
                // is despawned at the end of the frame, so no observer ever
                // meets a vanished child.
                insert_into(&mut access.commands, entity, Visibility::Hidden);
                access.retired.0.push(entity);
                let progress = expand.progress;
                let scene = storage_surface_scene(
                    disk,
                    filesystems,
                    &access.palette.inner,
                    signature,
                    progress,
                );
                let child = access.commands.spawn_scene(scene).id();
                attach_child(&mut access.commands, block, child);
            }
            None => {
                let scene =
                    storage_surface_scene(disk, filesystems, &access.palette.inner, signature, 0.0);
                let child = access.commands.spawn_scene(scene).id();
                attach_child(&mut access.commands, block, child);
            }
        }
    }
    for (entity, surface, mut expand) in access.surfaces.iter_mut() {
        if disks.iter().any(|disk| disk.device_id == surface.disk) {
            continue;
        }
        if expand.target == ExpandTarget::Collapsing {
            continue;
        }
        expand.target = ExpandTarget::Collapsing;
        let _ = entity;
    }
}

/// The refresh surface's queries.
///
/// One mutable access per component, and the markers read by entity.
///
/// Bevy forbids two `&mut` queries on the SAME component inside one system
/// (B0001): the donut ring and the NVMe gauge are separate entities but both
/// own a `ChartCanvas` and a `Text`. The markers are therefore read-only and
/// looked up by entity, so the system keeps one mutable borrow per component.
#[derive(SystemParam)]
struct StorageRefresh<'w, 's> {
    palette: Res<'w, WindowPalette>,
    donut_arcs: Query<'w, 's, &'static StorageDonutArc>,
    gauge_arcs: Query<'w, 's, &'static StorageGaugeArc>,
    donut_texts: Query<'w, 's, &'static StorageDonutText>,
    gauge_texts: Query<'w, 's, &'static StorageGaugeText>,
    canvases: Query<'w, 's, (Entity, &'static mut ChartCanvas)>,
    texts: Query<'w, 's, (Entity, &'static mut Text)>,
}

/// The surfaces whose motion settled are refreshed in place: the rings and
/// the gauge follow the folded numbers, so a telemetry tick never rebuilds
/// the card.
fn refresh_storage_surface(track: ShellTrack, mut access: StorageRefresh) {
    let shell = track.shell();
    let disks = crate::pages::performance::metrics::disks(shell).unwrap_or_default();
    for (entity, mut canvas) in &mut access.canvases {
        let Ok(marker) = access.donut_arcs.get(entity) else {
            continue;
        };
        let Some(disk) = disks.iter().find(|disk| disk.device_id == marker.disk) else {
            continue;
        };
        let Some(part) = disk
            .partitions
            .iter()
            .find(|part| part.name == marker.partition)
        else {
            continue;
        };
        let fraction = partition_fraction(part);
        canvas.content = ChartContent::Points {
            vertices: arc_vertices(RING_START, std::f32::consts::TAU * fraction.unwrap_or(0.0)),
            source: Vec2::splat(RING_DESIGN),
        };
        canvas.style.line = share_ink(fraction, &access.palette.inner);
    }
    for (entity, mut text) in &mut access.texts {
        let Ok(marker) = access.donut_texts.get(entity) else {
            continue;
        };
        let Some(disk) = disks.iter().find(|disk| disk.device_id == marker.disk) else {
            continue;
        };
        let Some(part) = disk
            .partitions
            .iter()
            .find(|part| part.name == marker.partition)
        else {
            continue;
        };
        let wanted = partition_readout(part);
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
    for (entity, mut canvas) in &mut access.canvases {
        let Ok(marker) = access.gauge_arcs.get(entity) else {
            continue;
        };
        let Some(disk) = disks.iter().find(|disk| disk.device_id == marker.0) else {
            continue;
        };
        let Some(gauge) = nvme_gauge_model(disk) else {
            continue;
        };
        let share = gauge
            .used_percent
            .map(|used| (used / 100.0).clamp(0.0, 1.0));
        canvas.content = ChartContent::Points {
            vertices: arc_vertices(GAUGE_START, GAUGE_SWEEP * share.unwrap_or(0.0)),
            source: Vec2::splat(RING_DESIGN),
        };
        canvas.style.line = band_ink(gauge.band, &access.palette.inner);
    }
    for (entity, mut text) in &mut access.texts {
        let Ok(marker) = access.gauge_texts.get(entity) else {
            continue;
        };
        let Some(disk) = disks.iter().find(|disk| disk.device_id == marker.0) else {
            continue;
        };
        let Some(gauge) = nvme_gauge_model(disk) else {
            continue;
        };
        let wanted = gauge_readout(&gauge);
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
}

/// The expand/collapse motion: the surface's height follows its typed state.
///
/// The list item and the surface it owns ride the SAME typed motion: one
/// component field, one per-frame advance, one retirement. A surface whose
/// layout has not measured it yet waits for the measurement — the motion
/// never collapses a surface to nothing it cannot measure.
fn animate_storage_surfaces(
    time: Res<bevy::time::Time>,
    mut retired: ResMut<StorageRetired>,
    mut surfaces: Query<(
        Entity,
        &mut StorageExpand,
        &mut Node,
        &bevy::ui::ComputedNode,
    )>,
) {
    let delta = time.delta_secs();
    for (entity, mut expand, mut node, measured) in &mut surfaces {
        let measured_natural = measured.size().y * measured.inverse_scale_factor();
        if expand.progress == 1.0 && expand.target == ExpandTarget::Expanding {
            // The motion is finished: the card owns its natural height again.
            node.height = Val::Auto;
            expand.natural_height = None;
            continue;
        }
        if expand.progress == 0.0 && expand.target == ExpandTarget::Collapsing {
            // The surface slid shut: it is retired at the end of the frame.
            retired.0.push(entity);
            continue;
        }
        if expand.natural_height.is_none() {
            if measured_natural <= 0.0 {
                // The layout has not measured this surface yet: the motion
                // waits for the measurement instead of guessing a height.
                continue;
            }
            expand.natural_height = Some(measured_natural);
        }
        advance_expand(&mut expand, delta);
        let natural = expand.natural_height.unwrap_or(0.0);
        node.height = Val::Px(natural * expand.progress);
    }
}

/// Flush the retired surfaces: the despawn runs in `Last`, so every observer
/// of this frame has already run against the live child.
fn flush_retired_surfaces(mut retired: ResMut<StorageRetired>, mut commands: Commands) {
    for entity in retired.0.drain(..) {
        commands.entity(entity).try_despawn();
    }
}

/// Register the storage surface's systems.
pub(crate) fn register(app: &mut bevy::app::App) {
    app.init_resource::<StorageRetired>();
    app.add_systems(
        bevy::app::Update,
        (sync_storage_surfaces, refresh_storage_surface),
    );
    app.add_systems(
        bevy::app::PostUpdate,
        animate_storage_surfaces.after(bevy::ui::UiSystems::PostLayout),
    );
    app.add_systems(bevy::app::Last, flush_retired_surfaces);
}
