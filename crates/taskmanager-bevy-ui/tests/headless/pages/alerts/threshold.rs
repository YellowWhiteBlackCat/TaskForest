//! test-intent: behavior
//!
//! The performance curve carries the alert threshold as a horizontal
//! calibration line: the line rides the curve's own scale and appears only
//! for an armed rule — a disabled rule's threshold is not watched, so
//! drawing its line would claim a bound that does not exist.

use super::*;
use crate::window::tests::scripted_frontend_app;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;

#[test]
fn the_threshold_line_rides_the_curve_scale() {
    let near = |actual: f32, expected: f32| {
        assert!(
            (actual - expected).abs() < 0.001,
            "{actual} is not within a thousandth of {expected}"
        );
    };
    near(threshold_line_top(100.0, 100.0), 0.0);
    near(threshold_line_top(100.0, 50.0), 50.0);
    near(threshold_line_top(100.0, 90.0), 10.0);
}

#[test]
fn only_an_armed_rule_calibrates_a_curve() {
    let shell = ShellApp::new();
    assert_eq!(
        curve_threshold(&shell, SystemCurve::Cpu),
        Some(90.0),
        "the default cpu rule calibrates the cpu curve"
    );
    assert_eq!(
        curve_threshold(&shell, SystemCurve::Network),
        None,
        "the throughput curve has no percentage threshold to mark"
    );
}

#[test]
fn the_performance_page_carries_the_alert_calibration_line() {
    let mut app = scripted_frontend_app();
    app.world_mut().resource_mut::<crate::app::Route>().page = crate::app::Page::Performance;
    app.update();

    let calibrations: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<ThresholdCalibration>>()
        .iter(app.world())
        .collect();
    assert!(
        !calibrations.is_empty(),
        "the performance page marks the armed alert threshold on the curve"
    );
}
