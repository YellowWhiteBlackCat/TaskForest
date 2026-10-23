//! Semantic action marks for the service lifecycle capsule group.
//!
//! The Bevy raster table (`taskmanager-assets`) ships no start/stop/restart
//! glyph, and adding one is a packaging change outside this page's lease — so
//! the three marks are composed from layout nodes instead of bitmaps: a play
//! wedge, a stop block and a restart ring. They are vector geometry, never
//! text codepoints, so the tofu law holds and no font has to cover them.
//!
//! The extent rides the palette's icon step (`rem(palette.icon_rem)`), so the
//! marks re-scale with the UI-size choice exactly like every other icon in
//! this frontend.

use bevy::ecs::component::Component;
use bevy::scene::{Scene, bsn};
use bevy::ui::prelude::{BackgroundColor, BorderColor, BorderRadius, Node, UiRect, Val, rem};

use crate::palette::UiPalette;

/// The lifecycle verbs the service action group owns.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ServiceActionGlyph {
    /// Start the unit.
    #[default]
    Start,
    /// Stop the unit.
    Stop,
    /// Restart the unit.
    Restart,
}

/// The mark's identity on the page: the semantic verb it stands for.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ServiceActionGlyphMark(pub(crate) ServiceActionGlyph);

/// The relative size of a lifecycle mark against the icon step — the same
/// proportion the reference surface uses for a toolbar icon.
const MARK_SCALE: f32 = 0.75;

/// The mark's ink per verb: starting is the app accent, stopping is the
/// destructive red, restarting is neutral body ink. A disabled control wears
/// the dim ink, so an inert action never looks armed.
#[must_use]
pub(crate) fn action_glyph_ink(
    glyph: ServiceActionGlyph,
    palette: &UiPalette,
    enabled: bool,
) -> bevy::color::Color {
    if !enabled {
        return palette.dim_color;
    }
    match glyph {
        ServiceActionGlyph::Start => palette.accent,
        ServiceActionGlyph::Stop => palette.danger_color,
        ServiceActionGlyph::Restart => palette.body_color,
    }
}

/// The lifecycle mark scene: one composed glyph, sized on the icon step.
pub(crate) fn action_glyph_scene(
    glyph: ServiceActionGlyph,
    palette: &UiPalette,
    enabled: bool,
) -> Box<dyn Scene> {
    let extent = palette.icon_rem * MARK_SCALE;
    let ink = action_glyph_ink(glyph, palette, enabled);
    match glyph {
        ServiceActionGlyph::Start => {
            // A play wedge: the right corners are fully rounded, the left ones
            // only softened, so the block reads as a right-pointing triangle.
            let radius = BorderRadius {
                top_left: bevy::ui::CornerRadius::all(Val::Rem(extent * 0.14)),
                bottom_left: bevy::ui::CornerRadius::all(Val::Rem(extent * 0.14)),
                top_right: bevy::ui::CornerRadius::all(Val::Rem(extent * 0.5)),
                bottom_right: bevy::ui::CornerRadius::all(Val::Rem(extent * 0.5)),
            };
            Box::new(bsn! {
                Node {
                    width: rem(extent),
                    height: rem(extent),
                    flex_shrink: 0.0,
                    border_radius: { radius },
                }
                BackgroundColor({ ink })
                ServiceActionGlyphMark({ glyph })
            })
        }
        ServiceActionGlyph::Stop => {
            // A stop block: a solid rounded square.
            let radius = BorderRadius::all(Val::Rem(extent * 0.2));
            Box::new(bsn! {
                Node {
                    width: rem(extent),
                    height: rem(extent),
                    flex_shrink: 0.0,
                    border_radius: { radius },
                }
                BackgroundColor({ ink })
                ServiceActionGlyphMark({ glyph })
            })
        }
        ServiceActionGlyph::Restart => {
            // A restart ring: a bordered circle whose center stays open, so the
            // panel behind shows through.
            let radius = BorderRadius::all(Val::Rem(extent * 0.5));
            let thickness = extent * 0.18;
            Box::new(bsn! {
                Node {
                    width: rem(extent),
                    height: rem(extent),
                    flex_shrink: 0.0,
                    border: UiRect::all(Val::Rem(thickness)),
                    border_radius: { radius },
                }
                BorderColor::all(ink)
                ServiceActionGlyphMark({ glyph })
            })
        }
    }
}
