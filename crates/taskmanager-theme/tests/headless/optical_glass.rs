use super::*;
use crate::fonts::ResolvedFonts;
use crate::theme::{HighContrast, LightDark, Skin};

fn full_capability(max_nested_layers: u8, max_blur_px: u32) -> OpticalGlassCapability {
    OpticalGlassCapability::reported_by_renderer(
        OpticalGlassOperations::COMPLETE,
        max_nested_layers,
        max_blur_px,
    )
}

fn theme(skin: Skin, mode: LightDark, hc: HighContrast) -> Theme {
    Theme::build(skin, mode, hc, ResolvedFonts::system_for(skin))
}

#[test]
fn specs_are_rem_authored_and_scale_only_at_the_renderer_boundary() {
    let theme = Theme::dark();
    let spec = theme
        .optical_glass_spec(GlassTier::Card)
        .expect("card glass is optical-capable design intent");

    assert_eq!(spec.blur_radius_rem, GlassTier::Card.blur_radius_rem());
    assert!(spec.refraction_strength_rem > 0.0);
    assert!(spec.refraction_width_rem > 0.0);
    assert!(spec.specular_width_rem > 0.0);
    assert!(spec.corner_radius_rem > 0.0);
    assert!(spec.chromatic_dispersion > 0.0);
    assert!(spec.frost_opacity > 0.0 && spec.frost_opacity <= 1.0);
    assert!(spec.grain_strength > 0.0 && spec.grain_strength <= 1.0);

    let logical = spec
        .resolve(OpticalGlassContext::new(14.0, 1.0))
        .expect("valid standard-scale context");
    assert_eq!(logical.blur_radius_px, 20.0);

    let hidpi = spec
        .resolve(OpticalGlassContext::new(18.0, 2.0))
        .expect("valid HiDPI context");
    assert_eq!(hidpi.blur_radius_px, spec.blur_radius_rem * 18.0 * 2.0);
    assert_eq!(
        hidpi.refraction_strength_px,
        spec.refraction_strength_rem * 18.0 * 2.0
    );
    assert_eq!(hidpi.corner_radius_px, spec.corner_radius_rem * 18.0 * 2.0);
}

#[test]
fn root_rem_size_survives_skin_mode_and_contrast_recomposition() {
    let mut theme = Theme::dark().with_root_rem_size(18.0);
    theme.set_skin(Skin::Windows);
    theme.set_mode(LightDark::EyeForest);
    theme.set_high_contrast(true);

    assert_eq!(theme.root_rem_size, 18.0);
    let spec = theme
        .optical_glass_spec(GlassTier::Card)
        .expect("card spec after recomposition");
    assert_eq!(
        spec.corner_radius_rem,
        theme.radius(RadiusScale::Large) / 18.0
    );
}

#[test]
fn every_skin_mode_and_contrast_has_valid_panel_specs() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            for hc in [HighContrast::Off, HighContrast::On] {
                let theme = theme(skin, mode, hc);
                assert!(
                    theme.optical_glass_spec(GlassTier::Scrim).is_none(),
                    "{skin:?}/{mode:?}/{hc:?}: a scrim is not a panel"
                );

                for tier in [GlassTier::Sidebar, GlassTier::Card, GlassTier::Modal] {
                    let spec = theme.optical_glass_spec(tier).unwrap_or_else(|| {
                        panic!("{skin:?}/{mode:?}/{hc:?}/{tier:?}: missing spec")
                    });
                    let geometry = spec
                        .resolve(OpticalGlassContext::new(theme.root_rem_size, 1.0))
                        .unwrap_or_else(|| {
                            panic!("{skin:?}/{mode:?}/{hc:?}/{tier:?}: invalid geometry")
                        });
                    assert!(
                        geometry.blur_radius_px > 0.0
                            && geometry.refraction_strength_px > 0.0
                            && geometry.refraction_width_px > 0.0
                            && geometry.specular_width_px > 0.0,
                        "{skin:?}/{mode:?}/{hc:?}/{tier:?}: every optical length is positive"
                    );
                    // One material authority: the GPU panel wears exactly
                    // the glass layer the painted fallback paints, so a tier
                    // cannot have two different opacities and a card cannot
                    // lose its elevation when real glass turns on.
                    assert_eq!(
                        spec.tint,
                        theme.glass_layer(tier).fill,
                        "{skin:?}/{mode:?}/{hc:?}/{tier:?}: the optical tint IS the tier's glass fill"
                    );
                    assert_eq!(
                        spec.frost_opacity, 1.0,
                        "{skin:?}/{mode:?}/{hc:?}/{tier:?}: the panel covers its backdrop; the optics live inside the sampled backdrop"
                    );
                }
            }
        }
    }
}

#[test]
fn tier_strength_is_ordered_and_modal_uses_high_quality() {
    let theme = Theme::dark();
    let sidebar = theme
        .optical_glass_spec(GlassTier::Sidebar)
        .expect("sidebar");
    let card = theme.optical_glass_spec(GlassTier::Card).expect("card");
    let modal = theme.optical_glass_spec(GlassTier::Modal).expect("modal");

    assert!(sidebar.blur_radius_rem > card.blur_radius_rem);
    assert!(modal.blur_radius_rem > card.blur_radius_rem);
    assert!(sidebar.refraction_strength_rem < card.refraction_strength_rem);
    assert!(card.refraction_strength_rem < modal.refraction_strength_rem);
    assert!(card.refraction_width_rem < modal.refraction_width_rem);
    assert!(card.chromatic_dispersion < modal.chromatic_dispersion);
    assert_eq!(sidebar.quality, OpticalGlassQuality::Balanced);
    assert_eq!(card.quality, OpticalGlassQuality::Balanced);
    assert_eq!(modal.quality, OpticalGlassQuality::High);
}

#[test]
fn capability_has_no_implicit_optical_default() {
    let capability = OpticalGlassCapability::UNSUPPORTED;
    assert_eq!(
        capability.first_missing_feature(),
        Some(OpticalGlassFeature::BackdropSampling)
    );
    for feature in OpticalGlassFeature::REQUIRED {
        assert!(!capability.supports(feature));
    }

    let theme = Theme::dark();
    let decision = theme.optical_glass_decision(
        GlassTier::Card,
        capability,
        OpticalGlassContext::new(14.0, 1.0),
    );
    assert_eq!(
        decision,
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Painted,
            reason: OpticalGlassUnavailableReason::MissingFeature(
                OpticalGlassFeature::BackdropSampling
            ),
        }
    );
    assert!(!decision.is_optical());
}

#[test]
fn backdrop_blur_without_the_complete_optical_chain_degrades_to_frosted() {
    let capability = OpticalGlassCapability::reported_by_renderer(
        OpticalGlassOperations {
            backdrop_sampling: true,
            blur: true,
            refraction: false,
            dispersion: false,
            specular: false,
            grain: false,
        },
        2,
        64,
    );
    let decision = Theme::dark().optical_glass_decision(
        GlassTier::Card,
        capability,
        OpticalGlassContext::new(14.0, 1.0),
    );
    assert_eq!(
        decision,
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Frosted,
            reason: OpticalGlassUnavailableReason::MissingFeature(OpticalGlassFeature::Refraction),
        }
    );
}

#[test]
fn full_capability_returns_physical_geometry_for_optical_rendering() {
    let decision = Theme::dark().optical_glass_decision(
        GlassTier::Card,
        full_capability(2, 64),
        OpticalGlassContext::new(14.0, 1.0),
    );
    let OpticalGlassDecision::Optical { spec, geometry } = decision else {
        panic!("the complete capability must resolve to optical glass");
    };
    assert_eq!(spec.quality, OpticalGlassQuality::Balanced);
    assert_eq!(geometry.blur_radius_px, 20.0);
    assert!(decision.is_optical());
}

#[test]
fn nesting_and_blur_limits_fail_closed_to_frosted() {
    let capability = full_capability(2, 20);
    let theme = Theme::dark();

    let second_layer = theme.optical_glass_decision(
        GlassTier::Card,
        capability,
        OpticalGlassContext::new(14.0, 1.0).with_nesting_depth(1),
    );
    assert!(second_layer.is_optical());

    let third_layer = theme.optical_glass_decision(
        GlassTier::Card,
        capability,
        OpticalGlassContext::new(14.0, 1.0).with_nesting_depth(2),
    );
    assert_eq!(
        third_layer,
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Frosted,
            reason: OpticalGlassUnavailableReason::NestedLayerLimitExceeded,
        }
    );

    let blur_limited = theme.optical_glass_decision(
        GlassTier::Card,
        full_capability(2, 19),
        OpticalGlassContext::new(14.0, 1.0),
    );
    assert_eq!(
        blur_limited,
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Frosted,
            reason: OpticalGlassUnavailableReason::BlurLimitExceeded,
        }
    );

    let hidpi_blur_limited = theme.optical_glass_decision(
        GlassTier::Card,
        full_capability(2, 39),
        OpticalGlassContext::new(14.0, 2.0),
    );
    assert_eq!(
        hidpi_blur_limited,
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Frosted,
            reason: OpticalGlassUnavailableReason::BlurLimitExceeded,
        }
    );
}

#[test]
fn high_contrast_headless_and_scrim_fallbacks_are_unconditional() {
    let capability = full_capability(2, 64);
    let context = OpticalGlassContext::new(14.0, 1.0);
    let hc = theme(Skin::Gnome, LightDark::Dark, HighContrast::On);

    assert_eq!(
        hc.optical_glass_decision(GlassTier::Modal, capability, context),
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Opaque,
            reason: OpticalGlassUnavailableReason::HighContrast,
        }
    );
    assert_eq!(
        hc.optical_glass_decision(GlassTier::Modal, capability, context.headless()),
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Opaque,
            reason: OpticalGlassUnavailableReason::HighContrast,
        }
    );

    let normal = theme(Skin::Gnome, LightDark::Dark, HighContrast::Off);
    assert_eq!(
        normal.optical_glass_decision(GlassTier::Card, capability, context.headless()),
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Painted,
            reason: OpticalGlassUnavailableReason::Headless,
        }
    );
    assert_eq!(
        normal.optical_glass_decision(GlassTier::Scrim, capability, context),
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Painted,
            reason: OpticalGlassUnavailableReason::ScrimNotOptical,
        }
    );
}

#[test]
fn invalid_scale_or_root_size_never_reaches_the_gpu() {
    let capability = full_capability(2, 64);
    let theme = Theme::dark();
    for context in [
        OpticalGlassContext::new(0.0, 1.0),
        OpticalGlassContext::new(14.0, 0.0),
        OpticalGlassContext::new(f32::NAN, 1.0),
        OpticalGlassContext::new(14.0, f32::INFINITY),
    ] {
        assert_eq!(
            theme.optical_glass_decision(GlassTier::Card, capability, context),
            OpticalGlassDecision::Fallback {
                material: OpticalGlassFallback::Painted,
                reason: OpticalGlassUnavailableReason::InvalidSpec,
            }
        );
    }
}

#[test]
fn the_complete_matrix_never_labels_hc_or_unsupported_renderers_as_optical() {
    let context = OpticalGlassContext::new(14.0, 1.0);
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let off = theme(skin, mode, HighContrast::Off);
            let on = theme(skin, mode, HighContrast::On);
            for tier in [GlassTier::Sidebar, GlassTier::Card, GlassTier::Modal] {
                assert!(
                    off.optical_glass_decision(tier, full_capability(2, 64), context)
                        .is_optical(),
                    "{skin:?}/{mode:?}/{tier:?}: complete capability is optical"
                );
                assert!(
                    !off.optical_glass_decision(tier, OpticalGlassCapability::UNSUPPORTED, context)
                        .is_optical(),
                    "{skin:?}/{mode:?}/{tier:?}: unsupported renderer must fall back"
                );
                assert_eq!(
                    on.optical_glass_decision(tier, full_capability(2, 64), context),
                    OpticalGlassDecision::Fallback {
                        material: OpticalGlassFallback::Opaque,
                        reason: OpticalGlassUnavailableReason::HighContrast,
                    },
                    "{skin:?}/{mode:?}/{tier:?}: HC must remain opaque"
                );
            }
        }
    }
}

/// A degraded pipeline still has to LOOK like the tier it belongs to. The
/// theme owns that answer, so a renderer never decides for itself what an
/// opaque Card is — and two frontends cannot degrade the same tier
/// differently.
#[test]
fn a_degraded_tier_keeps_its_own_material() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let theme = theme(skin, mode, HighContrast::Off);
            for tier in GlassTier::ALL {
                let layer = theme.glass_layer(tier);

                let painted = theme.glass_fallback_surface(tier, OpticalGlassFallback::Painted);
                assert_eq!(painted.fill, layer.fill, "{skin:?}/{mode:?}/{tier:?}");
                assert_eq!(painted.edge, layer.edge, "{skin:?}/{mode:?}/{tier:?}");
                assert_eq!(painted.shadow, layer.shadow, "{skin:?}/{mode:?}/{tier:?}");
                assert_eq!(
                    painted.backdrop,
                    GlassFallbackBackdrop::None,
                    "a painted fallback samples nothing behind it"
                );

                let frosted = theme.glass_fallback_surface(tier, OpticalGlassFallback::Frosted);
                assert_eq!(frosted.fill, layer.fill, "{skin:?}/{mode:?}/{tier:?}");
                assert_eq!(
                    frosted.backdrop,
                    GlassFallbackBackdrop::Blurred,
                    "a frosted fallback keeps the backdrop, blurred"
                );

                let opaque = theme.glass_fallback_surface(tier, OpticalGlassFallback::Opaque);
                assert_eq!(opaque.fill.a, 1.0, "{skin:?}/{mode:?}/{tier:?}");
                assert_eq!(opaque.edge, layer.edge, "{skin:?}/{mode:?}/{tier:?}");
                assert!(!opaque.sheen, "an opaque fallback drops the sheen");
                let sheen_expected = tier != GlassTier::Scrim;
                assert_eq!(painted.sheen, sheen_expected, "{skin:?}/{mode:?}/{tier:?}");
                assert_eq!(frosted.sheen, sheen_expected, "{skin:?}/{mode:?}/{tier:?}");
                assert_eq!(
                    opaque.backdrop,
                    GlassFallbackBackdrop::None,
                    "an opaque fallback samples nothing behind it"
                );
            }
        }
    }
}

/// High contrast is the reason an opaque fallback exists, so a high-contrast
/// theme must never ask for the optical treatment in the first place — and
/// the fallback it would degrade to must already be opaque and flat.
#[test]
fn a_high_contrast_theme_degrades_to_a_flat_opaque_surface() {
    for skin in Skin::ALL {
        for mode in LightDark::ALL {
            let theme = theme(skin, mode, HighContrast::On);
            let tier = GlassTier::Card;
            let decision = theme.optical_glass_decision(
                tier,
                OpticalGlassCapability::UNSUPPORTED,
                OpticalGlassContext::new(theme.root_rem_size, 1.0),
            );
            let OpticalGlassDecision::Fallback { material, .. } = decision else {
                panic!("high contrast must not ask for optical glass");
            };
            assert_eq!(material, OpticalGlassFallback::Opaque);

            let surface = theme.glass_fallback_surface(tier, material);
            assert_eq!(surface.fill.a, 1.0, "{skin:?}/{mode:?}");
            assert!(!surface.sheen, "{skin:?}/{mode:?}");
            assert_eq!(surface.backdrop, GlassFallbackBackdrop::None);
        }
    }
}
