//! test-intent: behavior
//!
//! The shared typed bands must decide Bevy's whole-group admissions on real
//! probe windows: the page slot, not a window-height constant, is classified.

use crate::widgets::layout_budget::PerformanceFrameBudget;
use taskmanager_ui_contract::presentation::vocabulary::{PerformanceVerticalRunway, VerticalSpace};

#[test]
fn the_budget_folds_measured_slots_into_the_shared_runway() {
    let floor = PerformanceFrameBudget::for_page_slot(1180.0, 380.0);
    let core = PerformanceFrameBudget::for_page_slot(1180.0, 480.0);
    let charts = PerformanceFrameBudget::for_page_slot(1180.0, 780.0);
    let generous = PerformanceFrameBudget::for_page_slot(1180.0, 1000.0);

    assert_eq!(floor.performance.vertical, PerformanceVerticalRunway::Floor);
    assert_eq!(floor.page.vertical_space, VerticalSpace::Constrained);
    assert!(!floor.admits_full_inventory());

    assert_eq!(core.performance.vertical, PerformanceVerticalRunway::Core);
    assert_eq!(core.page.vertical_space, VerticalSpace::Constrained);
    assert!(!core.admits_full_inventory());

    assert_eq!(
        charts.performance.vertical,
        PerformanceVerticalRunway::Charts
    );
    assert_eq!(charts.page.vertical_space, VerticalSpace::Standard);
    assert!(charts.admits_full_inventory());

    assert_eq!(
        generous.performance.vertical,
        PerformanceVerticalRunway::Charts
    );
    assert_eq!(generous.page.vertical_space, VerticalSpace::Generous);
    assert!(generous.admits_full_inventory());
}

#[test]
fn a_narrow_slot_never_admits_the_full_chart_inventory() {
    let narrow_tall = PerformanceFrameBudget::for_page_slot(700.0, 1400.0);
    assert_eq!(
        narrow_tall.performance.vertical,
        PerformanceVerticalRunway::Charts
    );
    assert!(!narrow_tall.admits_full_inventory());
}
