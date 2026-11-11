use super::*;

fn spec() -> CardRowWidthSpec {
    // Four measured facts in one shape's logical unit: card floor, gap,
    // bucket-label floor and overflow-action floor. The cap is product
    // content, not a layout number.
    CardRowWidthSpec::new(280.0, 8.0, 132.0, 176.0, 4)
}

#[test]
fn a_wide_slot_admits_the_whole_measured_card_grid() {
    // 4 × 280 + 3 × 8 = 1144. One pixel less refuses the fourth card.
    let slot = spec().row_width(4);
    assert_eq!(
        admit_card_row_width(slot, 4, spec(), false),
        CardRowWidthAdmission::Full { columns: 4 }
    );
    assert_eq!(
        admit_card_row_width(slot - 1.0, 4, spec(), false),
        CardRowWidthAdmission::Full { columns: 3 }
    );
}

#[test]
fn a_narrow_slot_degrades_the_whole_group_to_one_column() {
    let slot = spec().card_min_width + spec().gap * 2.0;
    assert_eq!(
        admit_card_row_width(slot, 3, spec(), false),
        CardRowWidthAdmission::SingleColumn,
        "the last column is refused as a whole, never clipped"
    );
    assert_eq!(admit_card_row_width(slot, 3, spec(), false).columns(), 1);
}

#[test]
fn a_truncated_bucket_header_reserves_its_own_whole_group_slot() {
    let header = spec().header_width(true);
    assert!(header > spec().header_width(false));
    // The bucket label alone fits, but the label + overflow action do not.
    let slot = header - 1.0;
    assert!(slot > spec().header_width(false));
    assert_eq!(
        admit_card_row_width(slot, 4, spec(), true),
        CardRowWidthAdmission::Degrade,
        "the header's whole fact group is charged before any card column"
    );
    assert_eq!(
        admit_card_row_width(slot, 4, spec(), false),
        CardRowWidthAdmission::SingleColumn,
        "without an overflow action the same slot holds one whole card"
    );
    // A slot wide enough for one card but narrower than the overflow header
    // is exactly the case the header verdict must refuse.
    let overflow_header = spec().header_width(true);
    assert!(overflow_header > spec().card_min_width);
    assert_eq!(
        admit_card_row_width(overflow_header - 1.0, 4, spec(), true),
        CardRowWidthAdmission::Degrade
    );
    assert_eq!(
        admit_card_row_width(overflow_header, 4, spec(), true),
        CardRowWidthAdmission::SingleColumn,
        "the whole header and one whole card fit at the boundary"
    );
}

#[test]
fn a_slot_below_one_card_or_with_degenerate_metrics_degrades() {
    assert_eq!(
        admit_card_row_width(spec().card_min_width - 1.0, 4, spec(), false),
        CardRowWidthAdmission::Degrade
    );
    assert_eq!(
        admit_card_row_width(f32::NAN, 4, spec(), false),
        CardRowWidthAdmission::Degrade
    );
    assert_eq!(
        admit_card_row_width(
            1200.0,
            4,
            CardRowWidthSpec::new(0.0, 8.0, 96.0, 88.0, 4),
            false
        ),
        CardRowWidthAdmission::Degrade
    );
}

#[test]
fn the_column_request_and_the_admission_spend_the_same_floor() {
    for slot in [0.0, 42.0, 279.0, 280.0, 568.0, 856.0, 1144.0, 1600.0] {
        let requested = card_row_columns(slot, spec());
        let admitted = admit_card_row_width(slot, requested, spec(), false);
        let expected = if slot < spec().card_min_width {
            0
        } else {
            requested.min(4)
        };
        assert_eq!(
            admitted.columns(),
            expected,
            "slot {slot}: request and verdict disagree"
        );
        assert_eq!(
            admitted.is_admitted(),
            slot >= spec().card_min_width,
            "slot {slot}: degraded verdict must be honest"
        );
    }
}
