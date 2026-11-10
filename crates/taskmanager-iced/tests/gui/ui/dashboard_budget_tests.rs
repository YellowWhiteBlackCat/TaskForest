// test-intent: behavior
//! The Iced dashboard spends the shared whole-group admission verdict. Only
//! its compact-summary decision and chart/chrome split stay local.

use super::*;
use taskmanager_ui_contract::presentation::card_flow::{CardFlowSpec, admit_card_groups_capped};

fn spec() -> CardFlowSpec {
    CardFlowSpec::new(CARD_HEIGHT_FLOOR, CARD_HEIGHT_CEILING).with_gap(GROUP_GAP)
}

fn available(height: f32, compact_summary: bool) -> f32 {
    height
        - if compact_summary {
            COMPACT_SUMMARY_LINE_HEIGHT
        } else {
            SUMMARY_CARD_HEIGHT
        }
        - GROUP_GAP
}

/// The dashboard's group count, card height and zero-group degrade all come
/// from the shared contract: no local division or clamp decides admission.
#[test]
fn dashboard_budget_spends_the_shared_card_flow_verdict() {
    for (width, height) in [
        (480.0, 254.0),
        (480.0, 360.0),
        (720.0, 480.0),
        (1280.0, 720.0),
        (1600.0, 360.0),
        (480.0, 1200.0),
    ] {
        let budget = DashboardBudget::resolve(width, height);
        let expected_summary = height < COMPACT_SUMMARY_HEIGHT || width < COMPACT_SUMMARY_WIDTH;
        let expected_columns = if width >= COMPACT_SUMMARY_WIDTH { 2 } else { 1 };
        let admitted = admit_card_groups_capped(
            available(height, expected_summary),
            expected_columns,
            spec(),
            max_metric_groups(expected_columns),
        );
        assert_eq!(budget.compact_summary, expected_summary);
        assert_eq!(budget.columns, admitted.columns);
        assert_eq!(budget.count, admitted.cards().min(DASHBOARD_METRIC_COUNT));
        assert!(budget.count <= DASHBOARD_METRIC_COUNT);
        if admitted.groups == 0 {
            assert_eq!(budget.chart_height, 0.0);
        } else {
            assert_eq!(
                budget.chart_height,
                (admitted.card_height - CARD_CHROME_HEIGHT)
                    .clamp(CHART_HEIGHT_FLOOR, CHART_HEIGHT_CEILING)
            );
        }
    }
}

/// A slot too short for one complete card group admits ZERO cards, never a
/// half row, and leaves no chart height behind.
#[test]
fn short_slot_degrades_to_zero_complete_groups() {
    let budget = DashboardBudget::resolve(720.0, 200.0);
    assert_eq!(budget.count, 0);
    assert_eq!(budget.chart_height, 0.0);
    assert!(budget.compact_summary);
}

/// Even at a tall ceiling the dashboard never exceeds the four-card product
/// cap; the shared contract's capped variant re-clamps the height.
#[test]
fn tall_slot_caps_at_four_cards_and_keeps_a_bounded_chart() {
    let budget = DashboardBudget::resolve(1280.0, 2000.0);
    assert_eq!(budget.columns, 2);
    assert_eq!(budget.count, DASHBOARD_METRIC_COUNT);
    assert!(budget.chart_height >= CHART_HEIGHT_FLOOR);
    assert!(budget.chart_height <= CHART_HEIGHT_CEILING);
}
