//! The window's outer edge highlight (BEVY-180).
//!
//! A window the compositor does not frame touches the screen edge with
//! nothing between it and the wallpaper. A one-pixel hairline in the
//! theme's border tone keeps that edge readable against a dark desktop
//! without drawing a visible frame — the same "faint outer edge" the
//! reference frontends show on their undecorated windows.
//!
//! The highlight is the shell's outermost node: it never takes layout
//! space, never intercepts a pointer, and never moves a control.

use bevy::color::Color;
use bevy::picking::Pickable;
use bevy::prelude::Visibility;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{BackgroundColor, BorderColor, Node, PositionType, UiRect, Val};

use crate::palette::{UiPalette, space_1, with_alpha};

/// The edge highlight's ink: the theme's border tone at a low alpha, so the
/// edge reads as a highlight rather than a frame.
const EDGE_ALPHA: f32 = 0.55;

/// The window edge's hairline width, in the theme's hairline step.
pub(crate) fn edge_width_px() -> f32 {
    space_1()
}

/// The edge highlight's ink for the live palette.
#[must_use]
pub(crate) fn edge_ink(palette: &UiPalette) -> Color {
    with_alpha(palette.border_color, EDGE_ALPHA)
}

/// Marker on the window's outer edge highlight.
///
/// The edge is identified by this marker, never by "the first node that
/// happens to carry a border" — other surfaces (the focus halo) also draw a
/// border, and a positional probe would bind to whichever the archetype
/// order happened to yield first.
#[derive(bevy::ecs::component::Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WindowEdgeHighlight;

/// The edge highlight scene: a transparent box whose only ink is its
/// one-pixel border.
pub(crate) fn edge_highlight_scene(palette: &UiPalette, visible: bool) -> Box<dyn Scene> {
    if visible {
        Box::new(visible_edge_scene(palette))
    } else {
        Box::new(hidden_edge_scene(palette))
    }
}

/// The edge highlight as it is drawn.
fn visible_edge_scene(palette: &UiPalette) -> impl Scene + use<> {
    let ink = edge_ink(palette);
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Px(0.0),
            border: UiRect::all(Val::Px(edge_width_px())),
        }
        BackgroundColor({ Color::NONE })
        WindowEdgeHighlight
        BorderColor {
            left: { ink },
            top: { ink },
            right: { ink },
            bottom: { ink },
        }
        Pickable::IGNORE
    }
}

/// The edge highlight while the window's own frame is present: the hairline
/// is not drawn, but the node stays in the tree so the shell's shape does
/// not change with the window's decoration mode.
fn hidden_edge_scene(palette: &UiPalette) -> impl Scene + use<> {
    let ink = edge_ink(palette);
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Px(0.0),
        }
        BackgroundColor({ Color::NONE })
        BorderColor {
            left: { ink },
            top: { ink },
            right: { ink },
            bottom: { ink },
        }
        Pickable::IGNORE
        Visibility::Hidden
    }
}
