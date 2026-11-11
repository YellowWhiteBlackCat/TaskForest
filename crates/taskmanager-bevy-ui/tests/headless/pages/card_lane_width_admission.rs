//! test-intent: behavior
//!
//! Elastic-layout proof for the Applications card lane's RIGHT edge and
//! bounded lower content. The shared height verdict admits whole group
//! blocks; the shared width verdict admits whole card columns INSIDE a
//! measured content cap. Residual lane space must stay outside the cards,
//! and a truncated bucket's overflow header must not bypass that cap.

use taskmanager_theme::{Theme, tokens::UiSize};
use taskmanager_ui_contract::presentation::card_flow::CardFlowAdmission;
use taskmanager_ui_contract::presentation::card_width::{
    CardRowWidthBudget, admit_card_row_width_budget, card_row_columns,
};

use crate::pages::processes::card_lane::{CardFlowMetrics, card_flow_budget};
use crate::palette::{space_8, ui_palette};

fn metrics() -> CardFlowMetrics {
    CardFlowMetrics::from_palette(&ui_palette(&Theme::dark(), UiSize::Standard))
}

fn palette() -> crate::palette::UiPalette {
    ui_palette(&Theme::dark(), UiSize::Standard)
}

fn budget(width: f32, columns: usize, overflow: bool) -> CardRowWidthBudget {
    let spec = metrics().width_spec();
    admit_card_row_width_budget(width, columns, spec, overflow)
}

#[test]
fn a_wide_slot_admits_a_capped_grid_and_leaves_the_residual_outside_the_cards() {
    let metrics = metrics();
    let spec = metrics.width_spec();
    let width = 3_000.0;
    let columns = card_row_columns(width, spec);
    let admitted = budget(width, columns, false);
    assert_eq!(columns, metrics.max_columns);
    assert_eq!(admitted.columns, metrics.max_columns);
    assert_eq!(admitted.content_width, spec.content_width_cap);
    assert_eq!(admitted.card_width, metrics.card_max_width_px);
    assert_eq!(
        admitted.residual_width,
        width - spec.content_width_cap,
        "extra slot width stays outside the bounded lower content group"
    );
    assert!(admitted.card_width >= metrics.card_min_width_px);
    assert!(admitted.card_width <= metrics.card_max_width_px);
}

#[test]
fn the_measured_budget_and_the_plan_bind_the_same_shared_cap() {
    let metrics = metrics();
    let spec = metrics.width_spec();
    let width = 3_000.0;
    let flow = card_flow_budget(width, 620.0, 0.0, &palette());
    assert_eq!(flow.columns, metrics.max_columns);
    assert_eq!(flow.row_width.content_width, spec.content_width_cap);
    assert_eq!(flow.row_width.card_width, metrics.card_max_width_px);
    assert!(flow.row_width.residual_width > 0.0);
    assert_eq!(
        flow.row_width_measurements().content_width_rem * metrics.rem_px,
        flow.row_width.content_width,
        "Bevy converts the shared width verdict once at its palette seam"
    );
    assert_eq!(
        flow.row_width_measurements().card_width_rem * metrics.rem_px,
        flow.row_width.card_width,
        "the rendered card width is the shared verdict, not a local ladder"
    );
}

#[test]
fn a_narrow_slot_degrades_the_whole_group_to_one_bounded_card() {
    let metrics = metrics();
    let width = metrics.card_min_width_px + metrics.group_gap_px * 2.0;
    let flow = card_flow_budget(width, 600.0, 0.0, &palette());
    assert!(
        flow.group_count() >= 1,
        "the height verdict admits whole groups"
    );
    let admitted = budget(width, 2, false);
    assert_eq!(admitted.columns, 1);
    assert_eq!(admitted.content_width, width);
    assert_eq!(admitted.card_width, width);
    assert_eq!(admitted.residual_width, 0.0);
    assert!(admitted.card_width >= metrics.card_min_width_px);
    assert!(admitted.card_width <= metrics.card_max_width_px);
}

#[test]
fn an_overflow_header_reserves_its_own_bounded_group_slot() {
    let metrics = metrics();
    let spec = metrics.width_spec();
    let header = spec.header_width(true);
    assert!(
        header < metrics.card_min_width_px,
        "the Bevy fixture's whole overflow header fits one card floor"
    );
    assert_eq!(
        budget(metrics.card_min_width_px - 1.0, 2, true),
        CardRowWidthBudget::degraded(),
        "the whole header fact group is charged before the card row"
    );
    assert_eq!(
        budget(metrics.card_min_width_px, 2, true).columns,
        1,
        "one whole card and its header fit at the card floor"
    );
    let wide = budget(3_000.0, metrics.max_columns, true);
    assert_eq!(wide.content_width, spec.content_width_cap);
    assert_eq!(
        wide.residual_width,
        3_000.0 - spec.content_width_cap,
        "the overflow header cannot spend residual lane space"
    );
}

#[test]
fn a_truncated_bucket_keeps_its_measured_wide_grid() {
    let palette = palette();
    let width = 1180.0 - space_8() * 2.0;
    let flow = card_flow_budget(width, 620.0, 0.0, &palette);
    assert!(flow.columns >= 3, "a 1180px window admits a wide card grid");
    assert_eq!(
        flow.width_columns(flow.columns + 1, true),
        flow.columns,
        "a truncated bucket paints every admitted card column"
    );
    assert!(flow.row_width.content_width <= width);
    assert_eq!(
        flow.row_width.content_width + flow.row_width.residual_width,
        width,
        "the bounded content group and its residual partition the measured slot"
    );
}

#[test]
fn a_slot_too_narrow_for_one_card_degrades_the_whole_lane() {
    let metrics = metrics();
    assert_eq!(
        budget(metrics.card_min_width_px - 1.0, 1, false),
        CardRowWidthBudget::degraded(),
        "a slot too narrow for one card's fact line degrades the whole lane"
    );
    assert_eq!(
        budget(0.0, 3, false),
        CardRowWidthBudget::degraded(),
        "an unmeasured slot never fabricates a card row"
    );
    assert!(!CardRowWidthBudget::degraded().is_admitted());
}

#[test]
fn the_focus_rung_requires_a_whole_width_admitted_card() {
    let metrics = metrics();
    let palette = palette();
    let focus_height =
        metrics.control_height_px + metrics.group_gap_px + metrics.focus_card_height_px;
    let dashboard_height = metrics.card_max_width_px;

    let below_width_floor = card_flow_budget(
        metrics.card_min_width_px - 1.0,
        focus_height,
        dashboard_height,
        &palette,
    );
    assert_eq!(
        below_width_floor.row_width,
        CardRowWidthBudget::degraded(),
        "the compact rung has no complete horizontal card budget"
    );
    assert_eq!(
        below_width_floor.admission,
        CardFlowAdmission::Dense,
        "vertical room cannot admit a card below its measured width floor"
    );

    let at_width_floor = card_flow_budget(
        metrics.card_min_width_px,
        focus_height,
        dashboard_height,
        &palette,
    );
    assert_eq!(
        at_width_floor.admission,
        CardFlowAdmission::FocusCard {
            card_height: metrics.focus_card_height_px,
        },
        "one whole focus card is admitted exactly at both measured floors"
    );
}
