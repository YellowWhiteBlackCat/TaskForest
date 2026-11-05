//! Dynamic refresh adapters for the Performance page.
//!
//! These systems own the marker-to-projection rewrite after a drain fold. They
//! deliberately consume the page's typed vocabulary rather than re-declaring
//! it: the projection remains the sole memory of devices, samples and alerts.

use bevy::ecs::entity::Entity;
use bevy::ecs::observer::On;
use bevy::ecs::query::{With, Without};
use bevy::ecs::system::{Commands, Query, Res, SystemParam};
use bevy::scene::CommandsSceneExt;
use bevy::ui::prelude::{Display, Node, Val};
use bevy::ui::widget::Text;
use taskmanager_shell::ShellApp;

use super::device_curves::storage::{ExpandTarget, StorageExpand};
use super::metrics::{
    core_usage_fill_pct, curve_badge_text, curve_samples, curve_wanted, disk_spare_warning_for,
    dyn_field_text,
};
use super::scene::blocks::block_scene;
use super::{
    CpuField, CurveBadge, CurveGate, DynBar, DynBlock, DynDiskSpareAlert, DynSection, DynText,
    PerformanceDeviceFocus, PerformanceFocus, SparkStrip, SystemCurve, section_keys,
};
use crate::app::ShellTrack;
use crate::drain::ShellProjectionFolded;
use crate::palette::UiPalette;
use crate::scene_attach::attach_child;
use crate::widgets::chart::{ChartCanvas, ChartContent, ChartSurface};
use crate::window::WindowPalette;

#[derive(SystemParam)]
pub(crate) struct PerformanceRefreshQueries<'w, 's> {
    /// Marked numeric texts.
    texts: Query<'w, 's, (&'static DynText, &'static mut Text)>,
    /// Marked core-fill bars.
    bars: Query<'w, 's, (&'static DynBar, &'static mut Node), Without<DynText>>,
    /// Marked disk-spare alerts.
    disk_alerts: Query<'w, 's, (&'static DynDiskSpareAlert, &'static mut Node), Without<DynBar>>,
    /// Curve strips with their canvas and chart surface.
    strips: Query<
        'w,
        's,
        (
            &'static SparkStrip,
            &'static mut ChartCanvas,
            &'static mut ChartSurface,
        ),
    >,
    /// Curve gate markers.
    gates: Query<'w, 's, (Entity, &'static CurveGate)>,
    /// Curve badges whose visibility and latest fact follow the projection.
    badges: Query<'w, 's, (Entity, &'static CurveBadge)>,
    /// Card section markers.
    sections: Query<'w, 's, (Entity, &'static DynSection)>,
    /// Device block markers.
    blocks: Query<'w, 's, (Entity, &'static DynBlock)>,
    /// The expand motion of the device list items: a storage device slides
    /// shut before it leaves the projection.
    expands: Query<'w, 's, &'static mut StorageExpand, With<DynBlock>>,
    /// Bare node query shared by the strip and gate adapters.
    nodes: Query<'w, 's, &'static mut Node, (Without<DynBar>, Without<DynDiskSpareAlert>)>,
}

/// The pages' data-refresh trigger consumer (see [`crate::drain`]): re-read
/// the folded shell and push new facts into the mounted tree. Runs only when
/// the drain folded batches — an idle frame never reaches this observer, and
/// a frame with the page unmounted finds no markers and does nothing.
pub(crate) fn refresh_on_fold(
    _fold: On<ShellProjectionFolded>,
    track: ShellTrack,
    palette: Res<WindowPalette>,
    focus: Res<PerformanceFocus>,
    device_focus: Res<PerformanceDeviceFocus>,
    mut surface: PerformanceRefreshQueries,
    mut commands: Commands,
) {
    let shell = track.shell();
    rewrite_texts(shell, &mut surface.texts);
    rewrite_core_bars(shell, &mut surface.bars);
    sync_disk_spare_alerts(shell, &mut surface.disk_alerts);
    sync_strips(shell, &mut surface.strips);
    sync_curve_badges(shell, &mut surface.badges, &mut surface.nodes);
    sync_card_gates(
        shell,
        focus.0,
        device_focus.0.curve().is_some(),
        &surface.gates,
        &mut surface.nodes,
    );
    sync_blocks(
        shell,
        &palette.inner,
        &surface.sections,
        &surface.blocks,
        &mut surface.expands,
        &mut commands,
    );
}

fn sync_disk_spare_alerts(
    shell: &ShellApp,
    alerts: &mut Query<(&DynDiskSpareAlert, &mut Node), Without<DynBar>>,
) {
    for (alert, mut node) in alerts.iter_mut() {
        let wanted = if disk_spare_warning_for(shell, &alert.0) {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != wanted {
            node.display = wanted;
        }
    }
}

/// Rewrite every marked text from the current projection, skipping values
/// that did not change (no change detection on identical facts).
fn rewrite_texts(shell: &ShellApp, texts: &mut Query<(&DynText, &mut Text)>) {
    for (field, mut text) in texts.iter_mut() {
        let wanted = dyn_field_text(shell, &field.0);
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
}

/// Keep each newest-value pill mounted only when the curve has a finite
/// observation. Visibility and text change together so a hidden gap can
/// never leave stale evidence behind.
fn sync_curve_badges(
    shell: &ShellApp,
    badges: &mut Query<(Entity, &CurveBadge)>,
    nodes: &mut Query<&mut Node, (Without<DynBar>, Without<DynDiskSpareAlert>)>,
) {
    for (entity, badge) in badges.iter() {
        let wanted = if curve_badge_text(shell, badge.0).is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if let Ok(mut node) = nodes.get_mut(entity)
            && node.display != wanted
        {
            node.display = wanted;
        }
    }
}

/// Rewrite each bar's fill width from the same folded observation its row's
/// numeric text renders. An unavailable observation collapses the fill to
/// zero — a missing fact is never painted as progress.
fn rewrite_core_bars(shell: &ShellApp, bars: &mut Query<(&DynBar, &mut Node), Without<DynText>>) {
    for (field, mut node) in bars.iter_mut() {
        let pct = match &field.0 {
            CpuField::Core(index) => core_usage_fill_pct(shell, *index),
            _ => 0.0,
        };
        let wanted = Val::Percent(pct);
        if node.width != wanted {
            node.width = wanted;
        }
    }
}

/// Publish the current window into each curve's canvas.
///
/// The strip's width is NOT a design constant: the canvas measures the slot
/// the layout gave the chart and projects the window against that
/// measurement, so the curve fills the card whatever the window does. Gaps
/// stay gaps — a missing observation is never joined across.
fn sync_strips(
    shell: &ShellApp,
    strips: &mut Query<(&SparkStrip, &mut ChartCanvas, &mut ChartSurface)>,
) {
    for (strip, mut canvas, mut chart) in strips.iter_mut() {
        let values = curve_samples(shell, strip.0);
        let ceiling = crate::pages::performance::scene::chart::curve_ceiling(strip.0, &values);
        chart.0 = values.len();
        canvas.content = ChartContent::Samples {
            values,
            ceiling,
            capacity: shell.history.capacity(),
        };
    }
}

/// Flip each card's gate with its curve's data existence (only the GPU card
/// can close), touching only nodes whose display actually changes.
fn sync_card_gates(
    shell: &ShellApp,
    focus: SystemCurve,
    show: bool,
    gates: &Query<(Entity, &CurveGate)>,
    nodes: &mut Query<&mut Node, (Without<DynBar>, Without<DynDiskSpareAlert>)>,
) {
    let active = if curve_wanted(shell, focus) {
        focus
    } else {
        SystemCurve::default()
    };
    for (entity, gate) in gates.iter() {
        let wanted = if show && curve_wanted(shell, gate.0) && gate.0 == active {
            Display::Flex
        } else {
            Display::None
        };
        if let Ok(mut node) = nodes.get_mut(entity)
            && node.display != wanted
        {
            node.display = wanted;
        }
    }
}

/// Reconcile each section's blocks with the projection's key list: despawn
/// keys that left, spawn keys that arrived. The projection is the only
/// memory of which devices exist — the page keeps no cache.
///
/// A storage device rides the SAME typed motion the storage surface owns:
/// the list item slides shut before it leaves and slides open when it
/// arrives, so a hot-plugged disk is never a jump.
fn sync_blocks(
    shell: &ShellApp,
    palette: &UiPalette,
    sections: &Query<(Entity, &DynSection)>,
    blocks: &Query<(Entity, &DynBlock)>,
    expands: &mut Query<&mut StorageExpand, With<DynBlock>>,
    commands: &mut Commands,
) {
    for (section_entity, section) in sections.iter() {
        let desired = section_keys(shell, section.0);
        for (block_entity, block) in blocks.iter() {
            if block.0 != section.0 {
                continue;
            }
            let wanted = desired.iter().any(|key| key == &block.1);
            if wanted {
                // A device that came back while it was sliding shut slides
                // open again.
                if let Ok(mut expand) = expands.get_mut(block_entity)
                    && expand.target == ExpandTarget::Collapsing
                {
                    expand.target = ExpandTarget::Expanding;
                }
                continue;
            }
            // The list item slides shut; the motion system retires it once
            // the slide reaches its end.
            if let Ok(mut expand) = expands.get_mut(block_entity) {
                if expand.target != ExpandTarget::Collapsing {
                    expand.target = ExpandTarget::Collapsing;
                }
                continue;
            }
            commands.entity(block_entity).despawn();
        }
        for key in desired {
            if blocks
                .iter()
                .any(|(_, block)| block.0 == section.0 && block.1 == key)
            {
                continue;
            }
            if let Some(scene) = block_scene(section.0, &key, shell, palette) {
                let child = commands.spawn_scene(scene).id();
                attach_child(commands, section_entity, child);
            }
        }
    }
}
