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

/// The overlay roles resolve to distinct product tiers and elevations, so no
/// frontend keeps a private tier/elevation table.
#[test]
fn overlay_roles_resolve_one_tier_and_one_elevation() {
    assert_eq!(GlassSurfaceRole::Chrome.tier(), GlassTier::Sidebar);
    assert_eq!(GlassSurfaceRole::Menu.tier(), GlassTier::Card);
    assert_eq!(GlassSurfaceRole::Toast.tier(), GlassTier::Card);
    assert_eq!(GlassSurfaceRole::Tooltip.tier(), GlassTier::Card);
    assert_eq!(GlassSurfaceRole::Dialog.tier(), GlassTier::Modal);
    assert_eq!(GlassSurfaceRole::InlinePlot.tier(), GlassTier::Card);

    assert_eq!(
        GlassSurfaceRole::Chrome.elevation(),
        SurfaceElevation::Inset
    );
    assert_eq!(
        GlassSurfaceRole::Menu.elevation(),
        SurfaceElevation::Floating
    );
    assert_eq!(
        GlassSurfaceRole::Toast.elevation(),
        SurfaceElevation::Floating
    );
    assert_eq!(
        GlassSurfaceRole::Tooltip.elevation(),
        SurfaceElevation::Floating
    );
    assert_eq!(
        GlassSurfaceRole::Dialog.elevation(),
        SurfaceElevation::Floating
    );
    assert_eq!(
        GlassSurfaceRole::InlinePlot.elevation(),
        SurfaceElevation::Inset
    );
}

/// Every floating role's shared surface paints its tier's layered shadow;
/// the one inset role paints the same material without a second elevation.
#[test]
fn every_overlay_role_uses_the_shared_surface_contract() {
    let theme = theme();
    for role in [
        GlassSurfaceRole::Chrome,
        GlassSurfaceRole::Menu,
        GlassSurfaceRole::Toast,
        GlassSurfaceRole::Tooltip,
        GlassSurfaceRole::Dialog,
        GlassSurfaceRole::InlinePlot,
    ] {
        let surface = CardSurface::for_role(theme.palette(), role);
        assert_eq!(surface.tier(), Some(role.tier()));
        assert_eq!(surface.elevation(), role.elevation());
        let mut painted = surface.render();
        let shadows = painted.style().box_shadow.as_ref();
        match role.elevation() {
            SurfaceElevation::Floating => assert_eq!(
                shadows,
                Some(&glass_shadow(&theme, role.tier())),
                "{role:?} must paint the shared layered shadow"
            ),
            SurfaceElevation::Inset => assert!(
                shadows.is_none_or(|layers| layers.is_empty()),
                "{role:?} must not stack a second elevation"
            ),
        }
    }
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

/// The GPUI binding is the Layered projection of the theme's own ladder, so
/// the renderer never re-derives the elevation ink. The `SingleQuad`
/// projection (and its `single_quad_elevation()` opacity) belongs to a
/// renderer that can only paint one shadow quad — a different renderer must
/// not borrow it.
#[test]
fn the_gpu_i_binding_is_the_shared_layered_projection() {
    let theme = theme();
    for tier in GlassTier::ALL {
        let layered = tier.projected_shadow(&theme, GlassShadowProjection::Layered);
        let single = tier.projected_shadow(&theme, GlassShadowProjection::SingleQuad);
        let painted = glass_shadow(&theme, tier);
        assert_eq!(painted.len(), 2, "GPUI paints every shared layer");
        assert_eq!(painted[0].color, hsla(layered.ambient.color));
        assert_eq!(painted[1].color, hsla(layered.contact.color));
        // The single-quad projection is a different renderer's degradation;
        // it must not be what GPUI paints. Two tiers are exempt from the
        // difference check, and both exemptions are the theme's own
        // decisions: the Modal tier's single-quad elevation opacity is 1.0
        // (so its projection numerically coincides with the layered one), and
        // the Scrim tier carries no shadow ink at all (both projections are
        // fully transparent).
        if tier.single_quad_elevation().opacity() < 1.0 && tier != GlassTier::Scrim {
            assert_ne!(
                painted[0].color,
                hsla(single.ambient.color),
                "the {tier:?} tier's single-quad elevation must not leak into the GPUI ladder"
            );
        }
    }
}

/// The chrome role wears the Sidebar tier and never casts an elevation
/// shadow: persistent chrome sits flush in the window, not floating above a
/// backdrop.
#[test]
fn persistent_chrome_paints_the_sidebar_tier_without_elevation() {
    let theme = theme();
    let mut chrome =
        CardSurface::unpadded_for_role(theme.palette(), GlassSurfaceRole::Chrome).render();
    assert_eq!(
        chrome.style().background.as_ref(),
        Some(&crate::theme_binding::glass_fill(
            &theme,
            GlassTier::Sidebar
        )),
        "the chrome surface fills with the shared Sidebar-tier gradient"
    );
    assert!(
        chrome
            .style()
            .box_shadow
            .as_ref()
            .is_none_or(|shadows| shadows.is_empty()),
        "persistent chrome must not cast a floating elevation shadow"
    );
}
