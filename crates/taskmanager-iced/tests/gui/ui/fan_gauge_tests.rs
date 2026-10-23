//! Headless proofs for the fan RPM gauge's compact-size scale-text policy.

use super::*;

/// The compact gauge must sit below the shared full-ladder threshold, or
/// the shared scale-text tier would keep the dense subdivision on it.
const _: () = assert!(
    FAN_RPM_COMPACT_GAUGE_HEIGHT < device_chart::SECONDARY_DEVICE_CHART_HEIGHT,
    "the compact gauge must sit below the shared full-ladder threshold"
);

#[test]
fn the_compact_fan_gauge_is_a_gauge_height() {
    // The compact (strip) presentation renders the RPM gauge as a gauge
    // frame, not as the full-height primary graph.
    let compact = fan_rpm_gauge_height(2400.0, true);
    assert_eq!(compact, FAN_RPM_COMPACT_GAUGE_HEIGHT);
    assert_ne!(
        compact,
        device_chart::DEVICE_CHART_HEIGHT,
        "the compact frame renders a gauge, not the full-height graph"
    );
    assert_eq!(
        fan_rpm_gauge_height(2400.0, false),
        device_chart::DEVICE_CHART_HEIGHT
    );
}

#[test]
fn the_compact_fan_gauge_hides_the_dense_scale_subdivision() {
    let compact = fan_rpm_gauge_height(2400.0, true);
    // At the compact gauge height the scale text keeps the MAIN ticks only
    // (the maximum and the baseline): the dense midpoint subdivision is
    // hidden, so a compact window shows no stacked scale text.
    assert_eq!(
        fan_rpm_scale_ticks(2400.0, compact),
        vec![2400.0, 0.0],
        "the compact gauge drops the dense subdivision"
    );
    // The full-height gauge keeps the whole ladder.
    assert_eq!(
        fan_rpm_scale_ticks(2400.0, fan_rpm_gauge_height(2400.0, false)),
        vec![2400.0, 1200.0, 0.0]
    );
}
