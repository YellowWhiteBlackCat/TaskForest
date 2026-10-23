//! The product-owned scrollbar rail (BEVY-175).
//!
//! A scroll container that shows no scroll affordance is a scroll container
//! the user cannot see. This module supplies the product's rail: a
//! **translucent floating track** that overlays the scroll area's trailing
//! edge, and a **capsule thumb** that rides it.
//!
//! The behavior is Bevy's official `bevy_ui_widgets::Scrollbar`: the rail
//! owns the widget, the thumb is the widget's `ScrollbarThumb`, and the
//! engine sizes and places the thumb from the scroll position — including
//! the press-and-drag gesture, which the widget's own drag observers own.
//!
//! The rail declares the *track*; the scroll target is a runtime fact (the
//! sibling `ScrollArea` is created by the same scene), so a small system
//! binds the two through the widget's own `Scrollbar` component.
//!
//! The axis is not a guess: the binding reads the scroll area's own
//! `overflow` and mounts a vertical rail for a vertical scroller, a
//! horizontal rail for a horizontal one. A vertical rail on a horizontal
//! scroller would be a lie about where the content moves.

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::query::{With, Without};
use bevy::ecs::system::{Commands, Query};
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    BackgroundColor, BorderRadius, Node, OverflowAxis, PositionType, UiRect, Val, rem,
};
use bevy::ui_widgets::{ControlOrientation, ScrollArea, Scrollbar, ScrollbarThumb};

use crate::palette::{UiPalette, composite_over, with_alpha};
use crate::scene_attach::insert_into;

/// The rail's width, in rem — the track rides the reading-size axis.
pub(crate) const RAIL_WIDTH_REM: f32 = 0.5;
/// The thumb's minimum length along the rail, in rem. A very long list
/// still shows a thumb the user can grab.
const MIN_THUMB_REM: f32 = 1.5;

/// The lane the rail occupies: a scroll area keeps this lane clear so no
/// row's trailing edge ever sits under the rail.
///
/// The lane is the scroll area's own inset plus the rail's width on the
/// trailing edge — the rail floats over that lane, so it never covers a
/// row's trailing pixels.
pub(crate) fn rail_lane(insets: UiRect) -> UiRect {
    UiRect::new(
        insets.left,
        Val::Rem(RAIL_WIDTH_REM),
        insets.top,
        insets.bottom,
    )
}

/// The horizontal lane: the same contract for a strip that scrolls sideways.
/// The lane rides the bottom edge, where the horizontal rail floats.
pub(crate) fn rail_lane_horizontal(insets: UiRect) -> UiRect {
    UiRect::new(
        insets.left,
        insets.right,
        insets.top,
        Val::Rem(RAIL_WIDTH_REM),
    )
}

/// Marker on the product-owned scrollbar rail.
///
/// The scene declares the rail; this marker is what the binding system
/// looks for when it resolves the rail's scroll target.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProductScrollbar;

/// The scrollbar rail: a translucent track that floats over the scroll
/// area's trailing edge, with the capsule thumb inside it.
///
/// The rail is a **sibling** of the scroll area (both children of the same
/// viewport wrapper) and is absolutely positioned inside that wrapper, so it
/// never scrolls with the content and never takes layout space away from
/// it.
pub(crate) fn scrollbar_rail_scene(palette: &UiPalette) -> impl Scene + use<> {
    let thumb = scrollbar_thumb_scene(palette);
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            width: rem(RAIL_WIDTH_REM),
            border_radius: BorderRadius::all(rem(RAIL_WIDTH_REM / 2.0)),
        }
        // The track floats over the content surface, so its tint is
        // pre-composed in the theme's own sRGB space: the rail then reads
        // the same in every blend space (BEVY-183).
        BackgroundColor({ composite_over(palette.content_bg, with_alpha(palette.border_color, 0.25)) })
        ProductScrollbar
        Children [ @{ thumb } ]
    }
}

/// The horizontal rail: the same contract on the horizontal axis. The track
/// rides the scroll area's bottom edge and the capsule thumb travels along
/// it, so a horizontally scrolling strip is as legible as a vertical one.
pub(crate) fn scrollbar_rail_scene_horizontal(palette: &UiPalette) -> impl Scene + use<> {
    let thumb = scrollbar_thumb_scene(palette);
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Px(0.0),
            height: rem(RAIL_WIDTH_REM),
            border_radius: BorderRadius::all(rem(RAIL_WIDTH_REM / 2.0)),
        }
        BackgroundColor({ composite_over(palette.content_bg, with_alpha(palette.border_color, 0.25)) })
        ProductScrollbar
        Children [ @{ thumb } ]
    }
}

/// The capsule thumb: the engine owns its size and position (it is the
/// scrollbar's `ScrollbarThumb`), the product owns its ink and its
/// capsule radius.
fn scrollbar_thumb_scene(palette: &UiPalette) -> impl Scene + use<> {
    let radius = palette.rem_px * RAIL_WIDTH_REM / 2.0;
    bsn! {
        BackgroundColor({ with_alpha(palette.dim_color, 0.7) })
        ScrollbarThumb {
            border_radius: BorderRadius::all(Val::Px(radius)),
            border: UiRect::default(),
        }
    }
}

/// The rails that still wait for their scroll target.
type UnboundRails<'w, 's> =
    Query<'w, 's, (Entity, &'static ChildOf), (With<ProductScrollbar>, Without<Scrollbar>)>;

/// Bind every rail to the scroll area it belongs to.
///
/// The rail and the scroll area are siblings under the same viewport
/// wrapper, so the target is the wrapper's `ScrollArea` child. A rail
/// without a scroll area is left alone: an unbound rail would silently
/// do nothing, and a missing target is a scene bug, not a runtime error.
pub(crate) fn bind_scrollbar_targets(
    mut commands: Commands,
    rails: UnboundRails<'_, '_>,
    siblings: Query<&Children>,
    areas: Query<&Node, With<ScrollArea>>,
) {
    for (entity, parent) in &rails {
        let Ok(children) = siblings.get(parent.parent()) else {
            continue;
        };
        let Some(target) = children.iter().find(|child| areas.contains(**child)) else {
            continue;
        };
        let Ok(node) = areas.get(*target) else {
            continue;
        };
        let min_thumb_length = palette_min_thumb_length();
        insert_into(
            &mut commands,
            entity,
            Scrollbar::new(*target, orientation_of(node), min_thumb_length),
        );
    }
}

/// The axis the scroll area actually scrolls on.
///
/// The rail's axis follows the scroll area's own `overflow`: a strip that
/// scrolls sideways gets a horizontal rail, everything else a vertical one.
/// The axis is read from the target, never guessed by the rail.
fn orientation_of(node: &Node) -> ControlOrientation {
    if node.overflow.x == OverflowAxis::Scroll && node.overflow.y != OverflowAxis::Scroll {
        ControlOrientation::Horizontal
    } else {
        ControlOrientation::Vertical
    }
}

/// The minimum thumb length in the engine's logical pixels.
///
/// The widget's contract is a pixel length (it sizes the thumb in the
/// scroll area's own units), so the rem-axis step is converted once, here.
fn palette_min_thumb_length() -> f32 {
    MIN_THUMB_REM * 16.0
}
