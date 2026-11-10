//! test-intent: behavior
//!
//! Elastic-layout proof for the Applications card lane's RIGHT edge. The
//! shared height verdict admits whole group blocks, but the width verdict is
//! the frontend's own duty: a group's lower content is admitted as a WHOLE,
//! so a narrow viewport must degrade the whole card row to one column before
//! any PID/CPU/memory fact line or the group's overflow action is clipped.

use taskmanager_theme::{Theme, tokens::UiSize};

use crate::pages::processes::card_lane::{
    CardFlowMetrics, GroupWidthAdmission, admit_group_width, card_columns, card_flow_budget,
};
use crate::palette::{space_8, ui_palette};

fn metrics() -> CardFlowMetrics {
    CardFlowMetrics::from_palette(&ui_palette(&Theme::dark(), UiSize::Standard))
}

#[test]
fn a_wide_slot_admits_the_whole_measured_card_grid() {
    let metrics = metrics();
    let width = 1000.0;
    let columns = card_columns(width, &metrics);
    assert!(columns >= 2, "a wide lane admits a grid, not one banner");
    assert_eq!(
        admit_group_width(width, columns, &metrics, false),
        GroupWidthAdmission::Full { columns },
        "the whole lower content group fits without a clipped column"
    );
}

#[test]
fn a_narrow_slot_degrades_the_whole_group_to_one_column() {
    let metrics = metrics();
    // The shared height verdict still admits groups; the width verdict is the
    // one that must refuse to spend a second column on this slot.
    let width = metrics.card_min_width_px + metrics.group_gap_px * 2.0;
    let budget = card_flow_budget(width, 600.0, &ui_palette(&Theme::dark(), UiSize::Standard));
    assert!(budget.groups >= 1, "the height verdict admits whole groups");
    assert_eq!(
        admit_group_width(width, 2, &metrics, false),
        GroupWidthAdmission::SingleColumn,
        "a narrow right edge admits ONE whole card, never a clipped second"
    );
}

#[test]
fn an_overflow_header_reserves_its_own_whole_group_slot() {
    let metrics = metrics();
    // The overflow action lives in the GROUP HEADER above the card row, so a
    // truncated bucket may still paint every card column when the cards fit.
    // Its own header line keeps the label and action readable through the
    // header's typed overflow clip; it does not consume a card-width slot.
    let width = metrics.card_min_width_px * 2.0 + metrics.group_gap_px;
    assert_eq!(
        admit_group_width(width, 2, &metrics, true),
        GroupWidthAdmission::Full { columns: 2 },
        "the header action stays in the header group, not beside the cards"
    );
    assert_eq!(
        admit_group_width(width, 2, &metrics, false),
        GroupWidthAdmission::Full { columns: 2 },
        "without the overflow action the same slot admits both cards"
    );
}

#[test]
fn a_truncated_bucket_keeps_its_measured_wide_grid() {
    let palette = ui_palette(&Theme::dark(), UiSize::Standard);
    let width = 1180.0 - space_8() * 2.0;
    let budget = card_flow_budget(width, 620.0, &palette);
    assert!(
        budget.columns >= 3,
        "a 1180px window admits a wide card grid"
    );
    assert_eq!(
        budget.width_columns(budget.columns + 1),
        budget.columns,
        "a truncated bucket paints every admitted card column"
    );
}

#[test]
fn a_slot_too_narrow_for_one_card_degrades_the_whole_lane() {
    let metrics = metrics();
    assert_eq!(
        admit_group_width(metrics.card_min_width_px - 1.0, 1, &metrics, false),
        GroupWidthAdmission::Degrade,
        "a slot too narrow for one card's fact line degrades the whole lane"
    );
    assert_eq!(
        admit_group_width(0.0, 3, &metrics, false),
        GroupWidthAdmission::Degrade,
        "an unmeasured slot never fabricates a card row"
    );
    assert_eq!(
        GroupWidthAdmission::Degrade.columns(),
        0,
        "zero admitted columns is the typed whole-group degradation"
    );
    assert_eq!(GroupWidthAdmission::SingleColumn.columns(), 1);
    assert_eq!(GroupWidthAdmission::Full { columns: 3 }.columns(), 3);
}
