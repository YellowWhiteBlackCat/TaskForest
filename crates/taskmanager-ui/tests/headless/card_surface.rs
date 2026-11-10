//! The shared glass-surface contract: one material, two elevations.
//!
//! These assertions are the GPUI-side proof that the surface primitive owns
//! the whole material: an elevated surface paints the shared two-layer
//! shadow, an inset surface paints the same material without a second
//! elevation, and the binding's shadow IS the theme's layered projection.

use super::*;
use crate::theme_binding::{glass_shadow, hsla};
use gpui::Styled;
use taskmanager_theme::{GlassShadowProjection, Theme};

fn theme() -> Theme {
    Theme::dark()
}

/// The elevated surface paints exactly the shared layered shadow.
#[test]
fn an_elevated_surface_paints_the_shared_layered_shadow() {
    let theme = theme();
    let mut surface = CardSurface::new(theme.palette())
        .glass(GlassTier::Card)
        .render();
    let painted = surface
        .style()
        .box_shadow
        .as_ref()
        .expect("an elevated glass surface must paint the shared shadow");
    assert_eq!(painted, &glass_shadow(&theme, GlassTier::Card));
    assert_eq!(
        painted.len(),
        2,
        "GPUI paints every shared layer, so the elevation keeps both"
    );
}

/// The binding is the theme's layered projection, not a second ladder.
#[test]
fn the_gpui_shadow_is_the_shared_layered_projection() {
    let theme = theme();
    let shared = GlassTier::Card.projected_shadow(&theme, GlassShadowProjection::Layered);
    let painted = glass_shadow(&theme, GlassTier::Card);
    assert_eq!(painted.len(), 2);
    assert_eq!(painted[0].color, hsla(shared.ambient.color));
    assert_eq!(painted[1].color, hsla(shared.contact.color));
    // The ambient layer is the wide soft one; the contact layer is the tight
    // one that keeps the edge attached to the backdrop.
    assert!(shared.ambient.blur_radius >= shared.contact.blur_radius);
    assert!(shared.ambient.offset_y >= shared.contact.offset_y);
}

/// An inset surface keeps the material and drops the elevation.
#[test]
fn an_inset_surface_keeps_the_material_without_a_second_shadow() {
    let theme = theme();
    let palette = theme.palette();
    assert_eq!(
        CardSurface::new(palette)
            .glass(GlassTier::Card)
            .fill(&palette),
        CardSurface::new(palette)
            .glass(GlassTier::Card)
            .inset()
            .fill(&palette),
        "the inset form paints the same glass material as the elevated one"
    );
    let mut inset = CardSurface::new(palette)
        .glass(GlassTier::Card)
        .inset()
        .render();
    let painted = inset.style().box_shadow.as_ref();
    assert!(
        painted.is_none_or(|shadows| shadows.is_empty()),
        "a nested surface must not stack a second elevation onto its parent"
    );
}

/// Every glass tier reaches the surface through the same shared material.
#[test]
fn every_glass_tier_paints_through_the_shared_contract() {
    let theme = theme();
    for tier in GlassTier::ALL {
        let mut surface = CardSurface::new(theme.palette()).glass(tier).render();
        let painted = surface
            .style()
            .box_shadow
            .as_ref()
            .expect("every elevated glass tier paints the shared shadow");
        assert_eq!(
            painted,
            &glass_shadow(&theme, tier),
            "the {tier:?} tier must not own a private shadow ladder"
        );
    }
}
