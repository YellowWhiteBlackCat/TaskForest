//! The frosted viewport the properties modal floats over.

use bevy::color::{Alpha, Color};
use bevy::ecs::component::Component;
use bevy::ecs::hierarchy::Children;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{BackgroundColor, Node, PositionType, percent};

use crate::palette::UiPalette;

/// The declared gaussian radius of the frosted viewport, in px.
///
/// Modal geometry is the documented pixel exception to the rem sizing axis: a
/// blur radius is a screen-space filter radius, not a type-ladder step, so it
/// deliberately does not ride the reading-size ladder.
pub(crate) const MODAL_BACKDROP_BLUR_RADIUS_PX: f32 = 12.0;

/// The frosted-viewport contract of a modal overlay.
///
/// Bevy 0.20's UI renderer has no backdrop-filter primitive, so the frost is
/// expressed as a declared geometry contract plus the theme's own translucent
/// scrim token: the viewport behind the modal stays visible — dimmed and
/// diffused — instead of being replaced by an opaque wall. Every ink comes
/// from a palette token; the radius is modal geometry.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ModalBackdrop {
    /// The declared gaussian radius of the frosted viewport, in px.
    pub(crate) blur_radius_px: f32,
}

/// The dim layer of the frost: the theme's own translucent scrim token.
fn scrim_layer(palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute }
        BackgroundColor({ palette.scrim })
    }
}

/// The diffusion layer of the frost: the elevated panel surface at the
/// scrim's own translucency, so the dimmed viewport reads as a milky plate
/// instead of a flat sheet of ink. Both the ink and the translucency come
/// from palette tokens — nothing here invents a colour.
fn frost_layer(palette: &UiPalette) -> impl Scene + use<> {
    let frost: Color = palette.panel_fill.with_alpha(palette.scrim.alpha());
    bsn! {
        Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute }
        BackgroundColor({ frost })
    }
}

/// The frosted viewport behind the modal panel.
///
/// The layer is absolutely positioned inside the overlay, so it covers the
/// whole viewport without taking part in the panel's centred layout.
pub(super) fn backdrop_scene(palette: &UiPalette) -> impl Scene + use<> {
    bsn! {
        Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute }
        ModalBackdrop { blur_radius_px: MODAL_BACKDROP_BLUR_RADIUS_PX }
        Children [
            @{ scrim_layer(palette) } --
            @{ frost_layer(palette) }
        ]
    }
}
