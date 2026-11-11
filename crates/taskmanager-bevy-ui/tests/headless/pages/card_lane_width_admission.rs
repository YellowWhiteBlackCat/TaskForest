//! test-intent: behavior
//!
//! Elastic-layout proof for the Applications card lane's RIGHT edge. The
//! shared height verdict admits whole group blocks, and the shared width verdict
//! admits whole card columns: a group's lower content is admitted as a WHOLE,
//! so a narrow viewport must degrade the whole card row to one column before
//! any PID/CPU/memory fact line or the group's overflow action is clipped.

use taskmanager_theme::{Theme, tokens::UiSize};
use taskmanager_ui_contract::presentation::card_width::{
    CardRowWidthAdmission, admit_card_row_width, card_row_columns,
};

use crate::pages::processes::card_lane::{CardFlowMetrics, card_flow_budget};
use crate::palette::{space_8, ui_palette};

fn metrics() -> CardFlowMetrics {
    CardFlowMetrics::from_palette(&ui_palette(&Theme::dark(), UiSize::Standard))
}

fn admit(
    width: f32,
    columns: usize,
    metrics: &CardFlowMetrics,
    has_overflow: bool,
) -> CardRowWidthAdmission {
    admit_card_row_width(width, columns, metrics.width_spec(), has_overflow)
}

#[test]
fn a_wide_slot_admits_the_whole_measured_card_grid() {
    let metrics = metrics();
    let width = 1000.0;
    let columns = card_row_columns(width, metrics.width_spec());
    assert!(columns >= 2, "a wide lane admits a grid, not one banner");
    assert_eq!(
        admit(width, columns, &metrics, false),
        CardRowWidthAdmission::Full { columns },
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
        admit(width, 2, &metrics, false),
        CardRowWidthAdmission::SingleColumn,
        "a narrow right edge admits ONE whole card, never a clipped second"
    );
}

#[test]
fn an_overflow_header_reserves_its_own_whole_group_slot() {
    let metrics = metrics();
    // The header is a separate whole group: its label and overflow action
    // are charged together before any card column is spent. A slot that
    // clips either fact degrades the lane instead of painting a half header.
    let spec = metrics.width_spec();
    let width = spec.header_width(true) - 1.0;
    assert_eq!(
        admit(width, 2, &metrics, true),
        CardRowWidthAdmission::Degrade,
        "the whole header fact group is charged before the card row"
    );
    assert_eq!(
        admit(width, 2, &metrics, false),
        CardRowWidthAdmission::Degrade,
        "the measured header-overflow slot is also below one card's floor"
    );
    assert_eq!(
        admit(spec.header_width(true), 2, &metrics, true),
        CardRowWidthAdmission::Degrade,
        "the measured header is narrower than one card, so the card floor still refuses it"
    );
    assert_eq!(
        admit(spec.card_min_width, 2, &metrics, true),
        CardRowWidthAdmission::SingleColumn,
        "at the card floor one whole card and its header are admitted"
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
        budget.width_columns(budget.columns + 1, true),
        budget.columns,
        "a truncated bucket paints every admitted card column"
    );
}

#[test]
fn a_slot_too_narrow_for_one_card_degrades_the_whole_lane() {
    let metrics = metrics();
    assert_eq!(
        admit(metrics.card_min_width_px - 1.0, 1, &metrics, false),
        CardRowWidthAdmission::Degrade,
        "a slot too narrow for one card's fact line degrades the whole lane"
    );
    assert_eq!(
        admit(0.0, 3, &metrics, false),
        CardRowWidthAdmission::Degrade,
        "an unmeasured slot never fabricates a card row"
    );
    assert_eq!(
        CardRowWidthAdmission::Degrade.columns(),
        0,
        "zero admitted columns is the typed whole-group degradation"
    );
    assert_eq!(CardRowWidthAdmission::SingleColumn.columns(), 1);
    assert_eq!(CardRowWidthAdmission::Full { columns: 3 }.columns(), 3);
}
