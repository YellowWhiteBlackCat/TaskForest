//! Semantic icon → bevy bitmap bridge (ADR-017 adapter boundary).
//!
//! **The tofu law**: this frontend never paints decoration through text
//! codepoints. A glyph only exists if the embedded faces guarantee its
//! coverage, and icon semantics never live in a font at all. Icons resolve
//! through [`taskmanager_icons::path`] — the same semantic table GPUI draws
//! from — and render from the checked-in white RGBA bitmaps derived by
//! `packaging/regenerate-ui-icons.sh`, tinted at draw time with the theme ink
//! a text sibling would inherit.
//!
//! The runtime shape mirrors the text contract in `crate::window`: scene
//! builders stay pure and emit `IconPlate` + `IconInk`; one insert-observer
//! joins them with the `IconPlates` handle store and stamps the bitmap.
//! A semantic id whose bitmap is missing degrades to an invisible node —
//! an honest absence, never a placeholder glyph.

use std::collections::HashMap;

#[cfg(test)]
#[path = "../tests/headless/icons_support.rs"]
mod icons_support;

use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::lifecycle::Add;
use bevy::ecs::observer::On;
use bevy::ecs::query::With;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::image::Image;
use bevy::math::Vec2;
use bevy::picking::Pickable;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::scene::{EntityCommandsSceneExt, Scene, bsn};
use bevy::text::{InlineBox, InlineBoxKind};
use bevy::ui::prelude::{Node, rem};
use bevy::ui::widget::{ImageNode, InlineImage};
use taskmanager_assets::UI_ICON_RGBA_SIZE;
use taskmanager_assets::ui_icon_rgba;
use taskmanager_icons::path;
use taskmanager_ui_contract::IconId;

use crate::palette::UiPalette;

/// The semantic ids this frontend draws. The list is exhaustive for the
/// product surface: navigation tabs, the trailing settings/alert affordances,
/// device-sidebar rows, and the table sort direction indicators.
pub(crate) const PLATE_ICONS: &[IconId] = &[
    IconId::Performance,
    IconId::Applications,
    IconId::Services,
    IconId::System,
    IconId::Startup,
    IconId::Users,
    IconId::History,
    IconId::Alert,
    IconId::Settings,
    IconId::Cpu,
    IconId::Memory,
    IconId::Disk,
    IconId::Network,
    IconId::Gpu,
    IconId::NavigateUp,
    IconId::NavigateDown,
];

/// The embedded bitmap bytes for one semantic icon, through the single
/// authority chain: `IconId → asset path → RGBA bitmap`. A `None` means the
/// bitmap table has no entry for a registered semantic id — a packaging gap
/// the registry test turns into a gate failure, and the renderer into an
/// honest skip.
pub(crate) fn icon_rgba(icon: IconId) -> Option<&'static [u8]> {
    ui_icon_rgba(path(icon))
}

/// One decoded bitmap in the bevy image store. Length is guaranteed by the
/// packaging script and re-checked here: a wrong-sized blob skips rather
/// than handing the GPU store malformed data.
fn decoded_bitmap(rgba: &'static [u8]) -> Option<Image> {
    let size = Extent3d {
        width: UI_ICON_RGBA_SIZE,
        height: UI_ICON_RGBA_SIZE,
        depth_or_array_layers: 1,
    };
    let expected = (UI_ICON_RGBA_SIZE * UI_ICON_RGBA_SIZE * 4) as usize;
    if rgba.len() != expected {
        return None;
    }
    Some(Image::new(
        size,
        TextureDimension::D2,
        rgba.to_vec(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ))
}

/// Handle store for every semantic icon this frontend draws, built once at
/// startup from the embedded bitmap table. Testable without a window: the
/// constructor takes only the image store.
#[derive(Resource, Default)]
pub(crate) struct IconPlates {
    plates: HashMap<IconId, Handle<Image>>,
}

impl IconPlates {
    /// Decode every registered id; ids without a usable bitmap are skipped
    /// and counted (surfaced on stderr once — a packaging gap, not a runtime
    /// condition).
    #[must_use]
    pub(crate) fn build(images: &mut Assets<Image>) -> Self {
        let mut plates = HashMap::new();
        let mut skipped = 0usize;
        for &icon in PLATE_ICONS {
            match icon_rgba(icon).and_then(decoded_bitmap) {
                Some(image) => {
                    plates.insert(icon, images.add(image));
                }
                None => skipped += 1,
            }
        }
        if skipped > 0 {
            eprintln!(
                "taskforest-b: {skipped} semantic icons have no bitmap plate; \
                 they draw as honest absences"
            );
        }
        let _ = skipped;
        Self { plates }
    }

    /// The store handle for one semantic icon, if its bitmap decoded.
    #[must_use]
    pub(crate) fn handle(&self, icon: IconId) -> Option<Handle<Image>> {
        self.plates.get(&icon).cloned()
    }
}

/// Marker naming the semantic icon a node draws. Pure scene builders emit it;
/// [`stamp_icon_plate`] is its only applier. The `Default` impl exists only
/// for the bsn! template mechanism — the seed is never a drawn identity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IconPlate(pub(crate) IconId);

impl Default for IconPlate {
    fn default() -> Self {
        Self(IconId::Cpu)
    }
}

/// The theme ink the icon inherits — the icon equivalent of a text sibling's
/// `TextColor`. Emitted at scene time where the palette is already in hand.
/// `Default` is the bsn! template seed only.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct IconInk(pub(crate) Color);

/// Observer: stamp the bitmap handle and tint as a plate lands. The icon
/// counterpart of `style_text_role` — pages never touch image assets. The
/// bitmap handle resolves only when the plate store is ready, so the
/// `ImageNode` surface is declared here as a deferred `apply_scene(bsn!)`
/// patch; the spawning scene declares the sized node and the two semantic
/// markers.
pub(crate) fn stamp_icon_plate(
    trigger: On<Add<IconPlate>>,
    plates: Option<Res<IconPlates>>,
    marks: Query<&IconPlate>,
    inks: Query<&IconInk>,
    present: Query<(), With<ImageNode>>,
    mut commands: Commands,
) {
    let entity = trigger.event().entity;
    if present.get(entity).is_ok() {
        return;
    }
    let ink = inks.get(entity).map(|ink| ink.0).unwrap_or_default();
    let icon = marks.get(entity).ok().map(|mark| mark.0);
    let image = icon
        .and_then(|icon| plates.as_ref().and_then(|plates| plates.handle(icon)))
        .unwrap_or_default();
    commands.entity(entity).apply_scene(bsn! {
        ImageNode { color: {ink}, image: {image} }
    });
}

/// Register the bridge: the plate-stamp observer. The handle store's startup
/// system is owned by the window composition's Startup chain (the store must
/// exist before any scene spawns); headless tests build it directly through
/// [`IconPlates::build`].
pub(crate) fn register(app: &mut bevy::app::App) {
    app.add_observer(stamp_icon_plate);
    app.add_observer(stamp_inline_icon);
}

/// Startup: decode the bundled bitmap table into the image store. Chained
/// before `spawn_app_shell` so a spawned plate always finds its store. The
/// store is `Option` because headless compositions without an image store
/// legitimately skip the bridge instead of failing validation.
pub(crate) fn build_icon_plates(images: Option<ResMut<Assets<Image>>>, mut commands: Commands) {
    let Some(mut images) = images else {
        return;
    };
    commands.insert_resource(IconPlates::build(&mut images));
}

/// The icon scene: a square carrying the plate + ink markers; the stamp
/// observer turns it into a tinted bitmap. Plate resolution is deferred to
/// that observer, so a missing bitmap is an invisible square — not a panic
/// and not a glyph.
///
/// The extent comes from the palette's icon step, never from a literal: the
/// node rides the rem axis, so the user's UI-size choice re-scales every
/// semantic icon with the reading size.
pub(crate) fn icon_scene(icon: IconId, palette: &UiPalette, ink: Color) -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            width: rem(palette.icon_rem),
            height: rem(palette.icon_rem),
            flex_shrink: 0.0,
        }
        IconPlate({ icon })
        IconInk({ ink })
    })
}

/// Marker naming the semantic icon a TEXT-FLOW entity draws. The entity is a
/// child of a text node: bevy_text reserves its `InlineBox` inside the text
/// layout, so the icon flows, clips and aligns with the glyphs as one line —
/// not a flex sibling that can be orphaned from its label. The `Default` impl
/// exists only for the bsn! template mechanism.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InlineIcon(pub(crate) IconId);

impl Default for InlineIcon {
    fn default() -> Self {
        Self(IconId::Cpu)
    }
}

/// The text-flow icon scene: an in-flow `InlineBox` child of a text entity
/// carrying the inline + ink markers. The stamp observer turns it into a
/// tinted [`InlineImage`]; a missing bitmap is zero reserved space — not a
/// panic and not a glyph. `Pickable::IGNORE` keeps the flow box from
/// intercepting clicks aimed at the button the text belongs to.
///
/// The flow box is the one engine-px seam: `bevy_text` measures an inline box
/// in logical pixels, so the extent is the palette's icon step resolved
/// against the live rem size — it still re-scales with the UI-size choice.
pub(crate) fn inline_icon_scene(icon: IconId, palette: &UiPalette, ink: Color) -> Box<dyn Scene> {
    Box::new(bsn! {
        InlineBox { kind: InlineBoxKind::InFlow, size: Vec2::splat(palette.icon_size_px()) }
        InlineIcon({ icon })
        IconInk({ ink })
        Pickable::IGNORE
    })
}

/// Observer: stamp the bitmap handle and tint as an inline icon lands. The
/// text-flow counterpart of [`stamp_icon_plate`] — the handle is runtime
/// state, the flow box geometry is already declared by the scene, and the
/// engine's `update_inline_image_boxes` keeps the box size on the bitmap.
pub(crate) fn stamp_inline_icon(
    trigger: On<Add<InlineIcon>>,
    plates: Option<Res<IconPlates>>,
    marks: Query<&InlineIcon>,
    inks: Query<&IconInk>,
    boxes: Query<&InlineBox>,
    mut commands: Commands,
) {
    let entity = trigger.event().entity;
    let ink = inks.get(entity).map(|ink| ink.0).unwrap_or_default();
    let icon = marks.get(entity).ok().map(|mark| mark.0);
    let image = icon
        .and_then(|icon| plates.as_ref().and_then(|plates| plates.handle(icon)))
        .unwrap_or_default();
    // `InlineImage` itself is NOT bsn-declarable in 0.20: its `Option<f32>`
    // extent fields have no template support upstream, so neither a field
    // patch nor a whole-value patch compiles. The declared `InlineBox` stays
    // the geometry authority; this observer only completes the async bitmap
    // payload on the already-declared entity — a typed stamp, not a second
    // scene path.
    let extent = boxes
        .get(entity)
        .map(|flow_box| flow_box.size.x)
        .unwrap_or_default();
    let plate = InlineImage {
        color: ink,
        image,
        width: Some(extent),
        height: Some(extent),
        ..InlineImage::default()
    };
    commands.entity(entity).insert(plate);
}
