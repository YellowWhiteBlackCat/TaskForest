use crate::FrontendShape;
use crate::presentation::load_tiers::{
    DECLARATIONS, EdgeMembership, LoadTier, LoadTierDecision, LoadTierEdges, load_tier,
    load_tier_declaration, load_tier_edges, tier_within,
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
fn the_two_cell_grid_shapes_paint_the_shared_band() {
    assert_eq!(
        load_tier_edges(FrontendShape::Iced),
        Some(LoadTierEdges::SHARED)
    );
    assert_eq!(
        load_tier_edges(FrontendShape::Tui),
        Some(LoadTierEdges::SHARED)
    );
    assert_eq!(LoadTierEdges::SHARED.warn, 60.0);
    assert_eq!(LoadTierEdges::SHARED.danger, 85.0);
    assert_eq!(
        LoadTierEdges::SHARED.membership,
        EdgeMembership::AtEdgeIsHigher
    );
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
fn bevy_keeps_its_registered_wider_bar_band() {
    let bevy = load_tier_edges(FrontendShape::Bevy).expect("Bevy paints a load-tier band");
    assert_eq!(bevy.warn, 80.0);
    assert_eq!(bevy.danger, 95.0);
    assert_eq!(bevy.membership, EdgeMembership::AboveEdgeOnly);
    let reason = match load_tier_declaration(FrontendShape::Bevy) {
        Some(LoadTierDecision::Own { reason, .. }) => reason,
        other => panic!("Bevy must register its own band, got {other:?}"),
    };
    assert!(
        !reason.trim().is_empty(),
        "a divergent band must carry its driver"
    );
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
fn the_bar_band_keeps_an_edge_value_in_the_lower_tier() {
    let bevy = load_tier_edges(FrontendShape::Bevy).expect("Bevy paints a load-tier band");
    assert_eq!(tier_within(bevy, 80.0), Some(LoadTier::Calm));
    assert_eq!(tier_within(bevy, 80.1), Some(LoadTier::Warning));
    assert_eq!(tier_within(bevy, 95.0), Some(LoadTier::Warning));
    assert_eq!(tier_within(bevy, 95.1), Some(LoadTier::Alarm));
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
