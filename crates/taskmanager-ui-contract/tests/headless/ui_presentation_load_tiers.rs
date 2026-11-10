use crate::FrontendShape;
use crate::presentation::load_tiers::{
    DECLARATIONS, LoadTier, LoadTierDecision, LoadTierEdges, load_tier, load_tier_declaration,
    load_tier_edges, tier_within,
};

#[test]
fn every_shape_declares_the_load_tier_axis_exactly_once() {
    for frontend in FrontendShape::ALL {
        let matches = DECLARATIONS
            .iter()
            .filter(|candidate| candidate.frontend == frontend)
            .count();
        assert_eq!(
            matches, 1,
            "{frontend:?} must answer the load-tier axis once"
        );
        assert!(load_tier_declaration(frontend).is_some());
    }
}

#[test]
fn every_shape_that_paints_a_load_tier_paints_the_shared_band() {
    assert_eq!(
        load_tier_edges(FrontendShape::Iced),
        Some(LoadTierEdges::SHARED)
    );
    assert_eq!(
        load_tier_edges(FrontendShape::Tui),
        Some(LoadTierEdges::SHARED)
    );
    assert_eq!(
        load_tier_edges(FrontendShape::Bevy),
        Some(LoadTierEdges::SHARED),
        "the per-core bar band is the product's load band, not a second one"
    );
    assert_eq!(LoadTierEdges::SHARED.warn, 60.0);
    assert_eq!(LoadTierEdges::SHARED.danger, 85.0);
    // No shape can answer the load-tier axis with a band of its own: every
    // shape that paints tiers resolves to exactly the one shared band.
    for frontend in FrontendShape::ALL {
        if let Some(edges) = load_tier_edges(frontend) {
            assert_eq!(
                edges,
                LoadTierEdges::SHARED,
                "{frontend:?} must paint the shared band, not a second one"
            );
        }
    }
}

#[test]
fn the_shape_without_load_tier_paint_declares_absence_not_a_number() {
    assert_eq!(
        load_tier_declaration(FrontendShape::Gpui),
        Some(LoadTierDecision::Absent {
            reason: "the GPUI per-core matrix tints each cell by CPU type \
                     (performance, efficient and low-power ink) and never by a \
                     utilization band"
        })
    );
    // An absent shape can never borrow the fold: it resolves to no band and no tier.
    assert_eq!(load_tier_edges(FrontendShape::Gpui), None);
    assert_eq!(load_tier(FrontendShape::Gpui, 99.0), None);
}

#[test]
fn the_shared_band_buckets_across_both_edges() {
    for (value, wanted) in [
        (0.0, LoadTier::Calm),
        (59.9, LoadTier::Calm),
        (60.0, LoadTier::Warning),
        (84.9, LoadTier::Warning),
        (85.0, LoadTier::Alarm),
        (100.0, LoadTier::Alarm),
    ] {
        assert_eq!(
            tier_within(LoadTierEdges::SHARED, value),
            Some(wanted),
            "shared band must classify {value}"
        );
    }
}

#[test]
fn an_edge_value_always_belongs_to_the_higher_band() {
    // The inclusive edge is a property of the fold, not a per-shape setting:
    // no declaration can put an edge value back into the lower tier, which is
    // precisely what used to let the same core read as a warning in one surface
    // and as calm in another. Any band the owner hands out obeys one rule.
    let band = LoadTierEdges {
        warn: 80.0,
        danger: 95.0,
    };
    assert_eq!(tier_within(band, 79.9), Some(LoadTier::Calm));
    assert_eq!(tier_within(band, 80.0), Some(LoadTier::Warning));
    assert_eq!(tier_within(band, 94.9), Some(LoadTier::Warning));
    assert_eq!(tier_within(band, 95.0), Some(LoadTier::Alarm));
    // The same reading cannot tier differently per shape: an edge value is
    // never calm anywhere the axis is painted, and is absent only where the
    // shape declared the axis away.
    for frontend in FrontendShape::ALL {
        assert_ne!(
            load_tier(frontend, 60.0),
            Some(LoadTier::Calm),
            "{frontend:?} may not read the shared warn edge as calm"
        );
    }
}

#[test]
fn out_of_range_readings_clamp_into_the_nearest_band() {
    assert_eq!(
        tier_within(LoadTierEdges::SHARED, -50.0),
        Some(LoadTier::Calm),
        "a negative reading is not a warning"
    );
    assert_eq!(
        tier_within(LoadTierEdges::SHARED, 250.0),
        Some(LoadTier::Alarm),
        "a reading above the scale is an alarm, not a mystery"
    );
}

#[test]
fn a_non_finite_reading_has_no_tier() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            tier_within(LoadTierEdges::SHARED, value),
            None,
            "absence is not a band; the shape paints its missing-value ink"
        );
    }
}
