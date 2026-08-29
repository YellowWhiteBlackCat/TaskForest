//! Platform compensation decision table (CORE-07) — host-independent.
//!
//! Every case runs on every host because the axis is an explicit input;
//! these tests are the paired-test requirement for the toolkit bindings:
//! `gpui.rs` and `iced.rs` only project what is pinned here.

use super::*;
use crate::tokens::{
    FONT_WEIGHT_BODY, FONT_WEIGHT_BOLD, FONT_WEIGHT_EXTRA_BOLD, FONT_WEIGHT_HEADER,
    FONT_WEIGHT_MEDIUM, FONT_WEIGHT_NORMAL,
};

/// The whole decision table over both axes: FreeType is the identity,
/// DirectWrite snaps only the fractional body weight.
#[test]
fn decision_table_over_both_axes() {
    let ladder = [
        FONT_WEIGHT_NORMAL,
        FONT_WEIGHT_BODY,
        FONT_WEIGHT_MEDIUM,
        FONT_WEIGHT_HEADER,
        FONT_WEIGHT_BOLD,
        FONT_WEIGHT_EXTRA_BOLD,
    ];
    for weight in ladder {
        assert_eq!(
            effective_weight(weight, WeightCompensationAxis::FreeType),
            weight,
            "FreeType keeps authored weights unchanged: {weight:?}"
        );
    }
    for weight in ladder {
        let expected = if weight.0 == 450.0 {
            Weight(500.0)
        } else {
            weight
        };
        assert_eq!(
            effective_weight(weight, WeightCompensationAxis::DirectWrite),
            expected,
            "DirectWrite compensates only the fractional body weight: {weight:?}"
        );
    }
}

/// The snap window is open at ±1 (strictly less than 1.0 away from 450) —
/// the exact window the gpui binding shipped before the policy moved here;
/// changing the window is a deliberate policy change, not a refactor.
#[test]
fn snap_window_edges() {
    for axis in WeightCompensationAxis::ALL {
        assert_eq!(
            effective_weight(Weight(449.0), axis),
            Weight(449.0),
            "distance 1.0 is outside the {axis:?} snap window"
        );
        assert_eq!(
            effective_weight(Weight(451.0), axis),
            Weight(451.0),
            "distance 1.0 is outside the {axis:?} snap window"
        );
    }
    assert_eq!(
        effective_weight(Weight(449.9), WeightCompensationAxis::DirectWrite),
        Weight(500.0),
        "distance 0.1 is inside the snap window"
    );
}

/// The host's target axis is one of the declared axes — the cfg seam cannot
/// invent a third classification.
#[test]
fn target_axis_is_a_declared_axis() {
    assert!(WeightCompensationAxis::ALL.contains(&WeightCompensationAxis::target()));
}
