use super::*;

fn spec() -> CardRowWidthSpec {
    // Reason row: four measured facts plus a shape-measured card ceiling
    // and product column cap. The 360px card cap is a CONTENT measurement,
    // not a theme token or a breakpoint. The fold derives the whole-row cap
    // so a wide slot cannot turn the four cards into empty banners.
    CardRowWidthSpec::from_measured_card_cap(280.0, 360.0, 8.0, 132.0, 176.0, 4)
}

fn wide_slot() -> f32 {
    3_000.0
}

#[test]
fn the_measured_card_cap_derives_the_whole_row_content_cap() {
    let spec = spec();
    let expected = 360.0 * 4.0 + 8.0 * 3.0;
    assert_eq!(spec.content_width_cap, expected);
    assert_eq!(spec.row_width_cap(4), expected);
    assert_eq!(card_row_columns(wide_slot(), spec), 4);
}

#[test]
fn a_wide_slot_admits_whole_columns_inside_the_bounded_content_group() {
    let spec = spec();
    let slot = wide_slot();
    let budget = admit_card_row_width(slot, card_row_columns(slot, spec), spec, false).budget();
    assert_eq!(budget.columns, 4);
    assert_eq!(budget.content_width, spec.content_width_cap);
    assert_eq!(budget.card_width, spec.card_max_width);
    assert_eq!(budget.residual_width, slot - spec.content_width_cap);
    assert!(budget.card_width >= spec.card_min_width);
    assert!(budget.card_width <= spec.card_max_width);
    assert!(budget.residual_width > 0.0);
    assert_eq!(
        budget.content_width + budget.residual_width,
        slot,
        "the residual stays outside the cards, never inside a stretched card"
    );
    assert_eq!(
        budget.content_width,
        budget.card_width * 4.0 + spec.gap * 3.0,
        "gaps are charged once between whole cards"
    );
}

#[test]
fn a_slot_just_wide_enough_for_four_floor_cards_refuses_a_fifth_column() {
    let spec = spec();
    let slot = spec.row_width(4);
    let budget = admit_card_row_width(slot, 5, spec, false).budget();
    assert_eq!(budget.columns, 4);
    assert_eq!(budget.content_width, slot);
    assert_eq!(budget.card_width, spec.card_min_width);
    assert_eq!(budget.residual_width, 0.0);
    let one_less = admit_card_row_width(slot - 1.0, 4, spec, false).budget();
    assert_eq!(one_less.columns, 3);
    assert!(one_less.card_width >= spec.card_min_width);
}

#[test]
fn a_narrow_slot_degrades_the_whole_group_to_one_bounded_card() {
    let spec = spec();
    let slot = spec.card_min_width + spec.gap * 2.0;
    let budget = admit_card_row_width(slot, 3, spec, false).budget();
    assert_eq!(budget.columns, 1);
    assert_eq!(budget.content_width, slot);
    assert_eq!(budget.card_width, slot);
    assert_eq!(budget.residual_width, 0.0);
    assert!(
        budget.card_width >= spec.card_min_width && budget.card_width <= spec.card_max_width,
        "one whole card stays inside its measured floor and ceiling"
    );
    assert!(
        budget.residual_width >= 0.0,
        "a narrow slot never reports negative residual space"
    );
}

#[test]
fn an_overflow_header_cannot_spend_the_residual_lane_space() {
    let spec = spec();
    let slot = wide_slot();
    let budget = admit_card_row_width(slot, 4, spec, true).budget();
    assert_eq!(budget.columns, 4);
    assert!(spec.header_width(true) <= budget.content_width);
    assert!(
        spec.header_width(true) <= spec.content_width_cap,
        "the whole header fact group is charged before residual lane space"
    );
    assert_eq!(
        budget.residual_width,
        slot - budget.content_width,
        "the overflow header cannot widen itself into the residual"
    );

    let oversized_header_spec = CardRowWidthSpec::new(
        spec.card_min_width,
        spec.card_max_width,
        spec.gap,
        1400.0,
        176.0,
        1,
        spec.card_max_width * 4.0 + spec.gap * 3.0,
    );
    assert_eq!(
        admit_card_row_width(wide_slot(), 1, oversized_header_spec, true).budget(),
        CardRowWidthBudget::degraded(),
        "a header group larger than the bounded content cap degrades the lane"
    );
}

#[test]
fn a_bounded_header_overflow_verdict_keeps_one_whole_card_at_the_floor() {
    let spec = spec();
    let overflow_header = spec.header_width(true);
    assert_eq!(
        admit_card_row_width(overflow_header - 1.0, 4, spec, true).budget(),
        CardRowWidthBudget::degraded()
    );
    let slot = overflow_header.max(spec.card_min_width);
    let at_header = admit_card_row_width(slot, 4, spec, true).budget();
    assert_eq!(at_header.columns, 1);
    assert_eq!(at_header.content_width, slot);
    assert!(at_header.card_width >= spec.card_min_width);
    assert!(at_header.card_width <= spec.card_max_width);
    assert_eq!(at_header.residual_width, 0.0);
    assert!(
        spec.header_width(true) <= at_header.content_width,
        "the whole header group is charged inside the bounded card row"
    );
}

#[test]
fn a_slot_below_one_card_or_with_degenerate_metrics_degrades() {
    let spec = spec();
    assert_eq!(
        admit_card_row_width(spec.card_min_width - 1.0, 4, spec, false).budget(),
        CardRowWidthBudget::degraded()
    );
    assert_eq!(
        admit_card_row_width(f32::NAN, 4, spec, false).budget(),
        CardRowWidthBudget::degraded()
    );
    assert_eq!(
        admit_card_row_width(
            1_200.0,
            4,
            CardRowWidthSpec::new(0.0, 360.0, 8.0, 96.0, 88.0, 4, 1_200.0),
            false,
        )
        .budget(),
        CardRowWidthBudget::degraded()
    );
    assert_eq!(
        admit_card_row_width(
            1_200.0,
            4,
            CardRowWidthSpec::new(280.0, 279.0, 8.0, 96.0, 88.0, 4, 1_200.0),
            false,
        )
        .budget(),
        CardRowWidthBudget::degraded(),
        "an inverted measured card cap is never painted"
    );
}

#[test]
fn the_column_request_and_the_bounded_admission_agree_at_every_floor() {
    let spec = spec();
    for slot in [
        0.0, 42.0, 279.0, 280.0, 568.0, 856.0, 1_144.0, 1_600.0, 3_000.0,
    ] {
        let requested = card_row_columns(slot, spec);
        let budget = admit_card_row_width(slot, requested, spec, false).budget();
        let expected = if slot < spec.card_min_width {
            0
        } else {
            requested.min(4)
        };
        assert_eq!(
            budget.columns, expected,
            "slot {slot}: request and verdict disagree"
        );
        assert_eq!(
            budget.is_admitted(),
            slot >= spec.card_min_width,
            "slot {slot}: degraded verdict must be honest"
        );
        if budget.is_admitted() {
            assert!(budget.card_width >= spec.card_min_width);
            assert!(budget.card_width <= spec.card_max_width);
            assert!(budget.content_width <= spec.content_width_cap.min(slot));
            assert!(spec.header_width(false) <= budget.content_width);
            assert!(budget.residual_width >= 0.0);
            assert_eq!(budget.content_width + budget.residual_width, slot);
        }
    }
}

#[test]
fn the_budget_helper_and_the_typed_verdict_publish_the_same_fold() {
    let spec = spec();
    for (slot, overflow) in [
        (280.0, false),
        (856.0, false),
        (wide_slot(), true),
        (0.0, true),
    ] {
        let requested = card_row_columns(slot, spec);
        assert_eq!(
            admit_card_row_width_budget(slot, requested, spec, overflow),
            admit_card_row_width(slot, requested, spec, overflow).budget(),
            "slot {slot}, overflow {overflow}: one fold, one verdict"
        );
    }
}
