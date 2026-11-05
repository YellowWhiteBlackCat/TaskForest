//! test-intent: behavior
//!
//! The optical renderer claim is a composition fact, not a palette constant.
//! With no published render adapter it must fail closed through the theme's
//! typed Painted fallback. With an adapter it may report only the operations
//! and limits the production material bridge actually implements; the gate's
//! GPU proof establishes that those operations run, while these pure tests
//! establish that the report cannot exceed its declared ceiling.

use taskmanager_theme::optical_glass::{
    OpticalGlassCapability, OpticalGlassContext, OpticalGlassDecision, OpticalGlassFeature,
    OpticalGlassOperations, OpticalGlassUnavailableReason,
};
use taskmanager_theme::{GlassTier, OpticalGlassFallback, Theme};

use super::{
    OPTICAL_GLASS_MAX_BLUR_PX, OPTICAL_GLASS_MAX_NESTED_LAYERS, OPTICAL_GLASS_PROVEN_OPERATIONS,
    optical_glass_capability,
};

fn ceiling() -> OpticalGlassCapability {
    OpticalGlassCapability::reported_by_renderer(
        OPTICAL_GLASS_PROVEN_OPERATIONS,
        OPTICAL_GLASS_MAX_NESTED_LAYERS,
        OPTICAL_GLASS_MAX_BLUR_PX,
    )
}

#[test]
fn no_live_adapter_never_reports_optical_capability() {
    let capability = optical_glass_capability(None);
    assert_eq!(capability, OpticalGlassCapability::UNSUPPORTED);
    assert_eq!(
        Theme::dark().optical_glass_decision(
            GlassTier::Card,
            capability,
            OpticalGlassContext::new(14.0, 1.0),
        ),
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Painted,
            reason: OpticalGlassUnavailableReason::MissingFeature(
                OpticalGlassFeature::BackdropSampling,
            ),
        }
    );
}

#[test]
fn the_production_ceiling_reports_only_its_declared_operations_and_limits() {
    let capability = ceiling();
    for feature in OpticalGlassFeature::REQUIRED {
        assert!(capability.supports(feature), "missing {feature:?}");
    }
    assert_eq!(
        capability,
        OpticalGlassCapability::reported_by_renderer(OpticalGlassOperations::COMPLETE, 1, 64,),
        "changing the renderer ceiling requires a fresh GPU proof and receipt"
    );

    let context = OpticalGlassContext::new(14.0, 2.0);
    for tier in [GlassTier::Sidebar, GlassTier::Card, GlassTier::Modal] {
        let spec = Theme::dark()
            .optical_glass_spec(tier)
            .expect("panel tier has an optical spec");
        let geometry = spec.resolve(context).expect("tier geometry resolves");
        assert!(
            geometry.blur_radius_px <= OPTICAL_GLASS_MAX_BLUR_PX as f32,
            "{tier:?} blur {radius:.1} px exceeds the proven {OPTICAL_GLASS_MAX_BLUR_PX} px ceiling",
            radius = geometry.blur_radius_px,
        );
    }
}

#[test]
fn nesting_depth_beyond_the_proven_limit_fails_closed() {
    let capability = ceiling();
    let theme = Theme::dark();
    let top_level = OpticalGlassContext::new(14.0, 1.0).with_nesting_depth(0);
    let nested = top_level.with_nesting_depth(OPTICAL_GLASS_MAX_NESTED_LAYERS);

    let OpticalGlassDecision::Optical { .. } =
        theme.optical_glass_decision(GlassTier::Card, capability, top_level)
    else {
        panic!("the one proven top-level layer must accept the production Card tier");
    };
    assert_eq!(
        theme.optical_glass_decision(GlassTier::Card, capability, nested),
        OpticalGlassDecision::Fallback {
            material: OpticalGlassFallback::Frosted,
            reason: OpticalGlassUnavailableReason::NestedLayerLimitExceeded,
        },
        "a second glass layer cannot be relabelled optical without recursive capture proof"
    );
}
