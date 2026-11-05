//! The painted fallback viewport the properties modal floats over.

use bevy::color::{Alpha, Color};
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{BackgroundColor, Node, PositionType, percent};

use crate::palette::UiPalette;

/// Marker for the painted fallback behind a modal overlay.
///
/// Bevy 0.20's ordinary UI renderer has no backdrop-filter primitive. Until
/// the offscreen optical pipeline is wired into production, this marker owns
/// only the painted scrim plus diffusion fill and deliberately carries no
/// fake blur radius.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ModalBackdrop;

/// The dim layer of the painted fallback: the theme's translucent scrim token.
fn scrim_layer(palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute }
        BackgroundColor({ palette.glass_scrim_fill })
    }
}

/// The painted diffusion layer: the elevated panel surface at the scrim's
/// own translucency, so the dimmed viewport reads as a milky plate instead of
/// a flat sheet of ink. This performs no backdrop sampling or blur; both the
/// ink and the translucency come from palette tokens.
fn painted_diffusion_layer(palette: &UiPalette) -> impl Scene + use<> {
    let diffusion: Color = palette
        .panel_fill
        .with_alpha(palette.glass_scrim_fill.alpha());
    bsn! {
        Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute }
        BackgroundColor({ diffusion })
    }
}

/// The painted fallback behind the modal panel.
///
/// The layer is absolutely positioned inside the overlay, so it covers the
/// whole viewport without taking part in the panel's centred layout.
pub(super) fn backdrop_scene(palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute }
        ModalBackdrop
        Children [
            @{ scrim_layer(palette) } --
            @{ painted_diffusion_layer(palette) }
        ]
    }
}
