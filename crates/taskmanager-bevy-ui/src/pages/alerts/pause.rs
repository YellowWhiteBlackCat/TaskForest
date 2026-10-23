//! The global telemetry-pause overlay.
//!
//! While the telemetry refresh is paused the surface is frozen, and a
//! frozen surface must say so: a translucent wash dims the content and a
//! diagonal PAUSED watermark crosses it. Without the overlay the numbers
//! would silently stand still and read as live.
//!
//! The overlay hangs under the shell's content slot — the region every page
//! renders into — so the freeze reads the same on all nine pages without
//! each page owning a copy of the treatment.

use crate::app::ContentSlot;
use crate::palette::UiPalette;
use crate::scene_attach::attach_child;
use crate::window::{Role, TextRole, WindowPalette};
use bevy::app::{App, PostUpdate};
use bevy::color::Alpha;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::Children;
use bevy::ecs::query::With;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res};
use bevy::math::Rot2;
use bevy::picking::Pickable;
use bevy::scene::{CommandsSceneExt, Scene, bsn};
use bevy::text::TextColor;
use bevy::ui::UiSystems;
use bevy::ui::prelude::{
    AlignItems, BackgroundColor, FlexDirection, JustifyContent, Node, Overflow, PositionType,
    UiTransform, px,
};
use bevy::ui::widget::Text;

/// The wash's alpha over the frozen content. A dim, not a blackout: the
/// surface stays readable so the user can still study the frozen values.
const FROST_ALPHA: f32 = 0.32;
/// The watermark's ink alpha. Faint enough to sit under the data, strong
/// enough to read as a watermark.
const WATERMARK_ALPHA: f32 = 0.18;
/// The watermark's tilt in radians — a shallow diagonal, not a 45° stamp.
const WATERMARK_ANGLE_RADIANS: f32 = -0.35;

/// Marker for the mounted pause overlay.
#[derive(Component, Clone, Default)]
pub(crate) struct PausedOverlay;

/// Marker for the watermark's text, whose ink the dimming system fades.
#[derive(Component, Clone, Default)]
pub(crate) struct PausedWatermark;

/// Mount or retire the pause overlay. The overlay follows the shell's own
/// telemetry-pause state, so every page shows the same freeze without
/// owning a second pause authority.
fn mount_pause_overlay(
    mut track: Option<NonSendMut<crate::app::FrontendTrack>>,
    palette: Option<Res<WindowPalette>>,
    slots: Query<Entity, With<ContentSlot>>,
    overlays: Query<Entity, With<PausedOverlay>>,
    mut commands: Commands,
) {
    let Ok(slot) = slots.single() else {
        return;
    };
    let paused = track
        .as_deref_mut()
        .is_some_and(|track| track.shell.paused());
    let existing: Vec<Entity> = overlays.iter().collect();
    if !paused {
        for entity in existing {
            commands.entity(entity).despawn();
        }
        return;
    }
    if !existing.is_empty() {
        return;
    }
    let Some(palette) = palette else {
        return;
    };
    let entity = commands.spawn_scene(scene(&palette.inner)).id();
    attach_child(&mut commands, slot, entity);
}

/// Fade the watermark's ink. The type-role observer owns the text color, so
/// the alpha is applied afterwards — the watermark reads as a watermark
/// instead of a headline shouted over the data.
fn fade_watermark(mut texts: Query<&mut TextColor, With<PausedWatermark>>) {
    for mut ink in &mut texts {
        ink.0 = ink.0.with_alpha(WATERMARK_ALPHA);
    }
}

/// Register the pause overlay: the mount system and the watermark's fade.
pub(crate) fn register(app: &mut App) {
    app.add_systems(
        PostUpdate,
        (
            mount_pause_overlay,
            fade_watermark.before(UiSystems::Prepare),
        )
            .chain(),
    );
}

/// The freeze treatment: a translucent wash over the content region with
/// the diagonal watermark across it. The overlay never intercepts a pointer
/// aimed at the page below.
pub(crate) fn scene(palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            right: px(0.0),
            bottom: px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            overflow: Overflow::clip(),
        }
        BackgroundColor({ palette.scrim.with_alpha(FROST_ALPHA) })
        PausedOverlay
        Pickable::IGNORE
        Children [
             Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
             }
             UiTransform { rotation: Rot2::radians(WATERMARK_ANGLE_RADIANS) }
             Pickable::IGNORE
             Children [
                  Text("PAUSED") TextRole(Role::Heading) PausedWatermark
             ]
        ]
    }
}
