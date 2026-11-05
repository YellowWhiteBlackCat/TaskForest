//! The floating-overlay root: every surface that floats above the page.
//!
//! The window owns two sibling UI trees. [`AppShellRoot`](super::AppShellRoot)
//! carries the resident shell — title bar, navigation rail, page slot and
//! status line. This root carries the transient surfaces: context menus,
//! modal dialogs, tooltips, toasts and the side panels.
//!
//! The separation is a rendering contract, not a convenience. A glass
//! surface samples the pixels that were already composited underneath it, so
//! the tree that holds the glass must not be part of the tree it samples.
//! Keeping the overlays in their own root lets the backdrop camera capture
//! the shell without ever capturing a floating surface, which removes the
//! self-sampling feedback loop by construction.

use bevy::ecs::component::Component;
use bevy::scene::{Scene, bsn};
use bevy::ui::{GlobalZIndex, Node, PositionType, percent, px};

/// The stacking band the floating-overlay root occupies.
///
/// The shell owns band 0. Popovers and tooltips declare their own higher
/// `GlobalZIndex`, so a menu opened inside a modal still stacks above the
/// modal it belongs to.
const OVERLAY_ROOT_Z: i32 = 50;

/// Marker for the root node that carries every floating surface.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct OverlayRoot;

/// The empty floating-overlay root.
///
/// The root spans the whole viewport so an absolutely positioned surface
/// resolves its origin against the window, exactly as it did when it was a
/// child of the shell root.
pub(crate) fn overlay_root_scene() -> Box<dyn Scene> {
    Box::new(bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(0.0),
            top: px(0.0),
            width: percent(100),
            height: percent(100),
            min_width: px(0.0),
            min_height: px(0.0),
        }
        GlobalZIndex({ OVERLAY_ROOT_Z })
        OverlayRoot
    })
}
