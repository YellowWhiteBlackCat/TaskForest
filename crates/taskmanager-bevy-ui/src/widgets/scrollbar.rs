//! The product-owned scrollbar rail (BEVY-175).
//!
//! A scroll container that shows no scroll affordance is a scroll container
//! the user cannot see; a container that shows one when nothing scrolls is a
//! lie. This module supplies the rail the product contract asks for: a
//! **narrow floating capsule** that rides the reserved lane at the scroll
//! area's trailing edge and paints **only while there is something to
//! scroll**.
//!
//! The contract is the shared rail geometry the GPUI component layer owns in
//! `taskmanager-ui::primitives::scrollbar::geometry` — a 12px lane, a 3px
//! capsule, a 48px minimum thumb, and a thumb that disappears when the
//! content fits the viewport; Iced carries the same numbers in its own
//! theme. This frontend mirrors them on the **rem axis**, so the rail rides
//! the user's UI-size step instead of freezing at those literal pixels.
//! Lifting the neutral math into the shared kit so all three frontends read
//! one authority is the recorded follow-up; nothing here re-derives the
//! thumb's geometry, which stays Bevy's own widget job.
//!
//! The behavior is Bevy's official `bevy_ui_widgets::Scrollbar`: the rail
//! owns the widget, the thumb is the widget's `ScrollbarThumb`, and the
//! engine sizes and places the thumb from the scroll position — including
//! the press-and-drag gesture, which the widget's own drag observers own.
//!
//! The widget's one rule the product cannot ship is its idle size: when the
//! content fits, the engine sizes the thumb to the **whole track**, so an
//! un-scrolled panel would paint a full-height bar. A second system reads
//! the bound scroll area's own measurement and takes the rail out of layout
//! whenever the axis does not overflow, which also returns the lane's
//! pointer hits to the content underneath.
//!
//! The rail declares the *lane*; the scroll target is a runtime fact (the
//! sibling `ScrollArea` is created by the same scene), so a small system
//! binds the two through the widget's own `Scrollbar` component.
//!
//! The axis is not a guess: the binding reads the scroll area's own
//! `overflow` and mounts a vertical rail for a vertical scroller, a
//! horizontal rail for a horizontal one. A vertical rail on a horizontal
//! scroller would be a lie about where the content moves.

use bevy::color::Color;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::{ChildOf, Children};
use bevy::ecs::query::{With, Without};
use bevy::ecs::system::{Commands, Query, Res};
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{
    BackgroundColor, BorderRadius, ComputedNode, Display, Node, OverflowAxis, PositionType, UiRect,
    Val,
};
use bevy::ui_widgets::{ControlOrientation, ScrollArea, Scrollbar, ScrollbarThumb};

use crate::palette::{UiPalette, composite_over, with_alpha};
use crate::scene_attach::insert_into;
use crate::window::WindowPalette;

/// The lane a scroll container keeps clear for the rail, in rem: the shared
/// contract's 12px hit strip, so no row's trailing edge ever sits under it.
pub(crate) const LANE_WIDTH_REM: f32 = 0.75;
/// The painted capsule, in rem: the shared contract's 3px thumb. The engine
/// sizes the thumb to the rail's own width, so the rail IS the capsule.
const CAPSULE_WIDTH_REM: f32 = 0.1875;
/// The capsule's inset at each end of the track, in rem. It never butts into
/// the card's rounded corners.
const CAPSULE_END_INSET_REM: f32 = 0.25;
/// The shortest capsule, in rem: the shared contract's 48px minimum, so a
/// very long list still shows a thumb the user can grab.
const MIN_THUMB_REM: f32 = 3.0;
/// The capsule's ink: the theme foreground at the shared idle alpha.
const CAPSULE_ALPHA: f32 = 0.30;
/// The overflow a rail will not paint for: sub-pixel content overshoot is
/// not something the user can scroll.
const SCROLL_TOLERANCE_PX: f32 = 0.5;

/// The lane's inset on a vertical rail's trailing edge.
///
/// The lane is the scroll area's own inset plus the rail's width on the
/// trailing edge — the rail floats over that lane, so it never covers a
/// row's trailing pixels.
pub(crate) fn rail_lane(insets: UiRect) -> UiRect {
    UiRect::new(
        insets.left,
        Val::Rem(LANE_WIDTH_REM),
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
        Val::Rem(LANE_WIDTH_REM),
    )
}

/// Marker on the product-owned scrollbar rail.
///
/// The scene declares the rail; this marker is what the binding system
/// looks for when it resolves the rail's scroll target.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProductScrollbar;

/// The rem-to-logical-pixel step for a length on the rail's own axis.
///
/// The widget's thumb contract is a pixel length (it sizes the thumb in the
/// scroll area's own units), so the rem-axis step is converted once, here,
/// against the live palette rather than a literal base size.
fn rem_px(palette: &UiPalette) -> f32 {
    palette.rem_px
}

/// The capsule's offset inside the lane: it sits centered in the strip the
/// scroll area reserved, exactly as the shared rail centers its thumb.
fn capsule_offset() -> Val {
    Val::Rem((LANE_WIDTH_REM - CAPSULE_WIDTH_REM) / 2.0)
}

/// The vertical rail: the capsule lane that floats over the scroll area's
/// trailing edge.
///
/// The rail is a **sibling** of the scroll area (both children of the same
/// viewport wrapper) and is absolutely positioned inside that wrapper, so it
/// never scrolls with the content and never takes layout space away from
/// it. It starts out of layout: the visibility system admits it only once
/// the bound scroll area proves the axis overflows.
pub(crate) fn scrollbar_rail_scene(palette: &UiPalette) -> impl Scene + use<> {
    let thumb = scrollbar_thumb_scene(palette);
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            right: capsule_offset(),
            top: Val::Rem(CAPSULE_END_INSET_REM),
            bottom: Val::Rem(CAPSULE_END_INSET_REM),
            width: Val::Rem(CAPSULE_WIDTH_REM),
        }
        // The overlay rail paints no track: the capsule is the affordance,
        // the same rule the shared rail's overlay mode applies.
        BackgroundColor({ Color::NONE })
        ProductScrollbar
        Children [ @{ thumb } ]
    }
}

/// The horizontal rail: the same contract on the horizontal axis. The lane
/// rides the scroll area's bottom edge and the capsule travels along it, so
/// a horizontally scrolling strip is as legible as a vertical one.
pub(crate) fn scrollbar_rail_scene_horizontal(palette: &UiPalette) -> impl Scene + use<> {
    let thumb = scrollbar_thumb_scene(palette);
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            left: Val::Rem(CAPSULE_END_INSET_REM),
            right: Val::Rem(CAPSULE_END_INSET_REM),
            bottom: capsule_offset(),
            height: Val::Rem(CAPSULE_WIDTH_REM),
        }
        BackgroundColor({ Color::NONE })
        ProductScrollbar
        Children [ @{ thumb } ]
    }
}

/// The capsule thumb: the engine owns its length and position (it is the
/// widget's `ScrollbarThumb`), the product owns its ink and its capsule
/// radius. The ink is pre-composed in the theme's own sRGB space so the
/// capsule reads the same in every blend space (BEVY-183).
fn scrollbar_thumb_scene(palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        BackgroundColor({ composite_over(palette.content_bg, with_alpha(palette.body_color, CAPSULE_ALPHA)) })
        ScrollbarThumb {
            border_radius: BorderRadius::all(Val::Rem(CAPSULE_WIDTH_REM / 2.0)),
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
    palettes: Res<WindowPalette>,
    rails: UnboundRails<'_, '_>,
    siblings: Query<&Children>,
    areas: Query<&Node, With<ScrollArea>>,
) {
    let min_thumb_length = MIN_THUMB_REM * rem_px(&palettes.inner);
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
        insert_into(
            &mut commands,
            entity,
            Scrollbar::new(*target, orientation_of(node), min_thumb_length),
        );
    }
}

/// Take every rail out of the picture when its axis has nothing to scroll.
///
/// The engine sizes an idle thumb to the whole track, so this is the rule
/// that keeps a full-height grey pole off a panel that fits. The measurement
/// is the scroll area's own: the visible box minus its reserved lane against
/// the content it holds, on the axis the rail drives.
pub(crate) fn sync_rail_visibility(
    mut rails: Query<(&mut Node, &Scrollbar)>,
    areas: Query<&ComputedNode>,
) {
    for (mut rail, scrollbar) in &mut rails {
        let shown = areas
            .get(scrollbar.target)
            .is_ok_and(|area| axis_overflows(area, scrollbar.orientation));
        let display = if shown { Display::Flex } else { Display::None };
        if rail.display != display {
            rail.display = display;
        }
    }
}

/// Whether the scroll area's content overshoots its visible box on `axis`.
fn axis_overflows(area: &ComputedNode, orientation: ControlOrientation) -> bool {
    let scale = area.inverse_scale_factor;
    let visible = (area.size() - area.scrollbar_size) * scale;
    let content = area.content_size() * scale;
    let overshoot = match orientation {
        ControlOrientation::Vertical => content.y - visible.y,
        ControlOrientation::Horizontal => content.x - visible.x,
    };
    overshoot > SCROLL_TOLERANCE_PX
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

#[cfg(test)]
#[path = "../../tests/headless/widgets/scrollbar.rs"]
mod tests;
