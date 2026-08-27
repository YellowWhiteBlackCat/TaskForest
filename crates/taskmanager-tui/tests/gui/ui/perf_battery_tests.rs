use super::*;
use taskmanager_application::{BatteryInfo, PowerSupplySnapshot};

/// A battery whose charge-% history has >=2 samples renders a real sparkline
/// (a ramp block) on its trend line; a battery with no history renders the
/// dotted placeholder. The trend line sits right after the header (index 1).
#[test]
fn battery_trend_line_matches_that_batterys_own_history_window() {
    // Record two power-supply snapshots for "BAT0" so its window has >=2
    // samples. Batteries live on the power event, not the system snapshot.
    let mut shell = taskmanager_shell::ShellApp::new();
    let mut battery = BatteryInfo::new("BAT0", Default::default());
    battery.apply_scalar_observations(taskmanager_application::BatteryScalarObservations {
        capacity_pct: taskmanager_application::ScalarObservation::available(80, 1),
        ..Default::default()
    });
    let snapshot = PowerSupplySnapshot {
        batteries: vec![battery],
        ..PowerSupplySnapshot::default()
    };
    let system = taskmanager_application::SystemSnapshot::default();
    taskmanager_shell::fixture::record_demo_history_frame(
        &mut shell,
        &system,
        Some(&snapshot),
        None,
    );
    taskmanager_shell::fixture::record_demo_history_frame(
        &mut shell,
        &system,
        Some(&snapshot),
        None,
    );
    let history = &shell.history;
    // A constant charge-% window resolves and trends to a flat mid-ramp.
    let window = history.battery_capacity_pct_for("BAT0");
    assert_eq!(window.len(), 2, "two power-supply snapshots recorded");
    assert!(
        super::super::sparkline::test_support::device_trend(&window).contains('▅'),
        "constant charge-% → flat mid-ramp"
    );

    // The trend line in battery_lines is line index 1 (right after header).
    let known = battery_lines(
        &[BatteryInfo::new("BAT0", Default::default())],
        history,
        TuiTheme::default(),
        60,
    );
    let trend_text: String = known[1]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(
        trend_text.contains('▅'),
        "known battery renders a ramp-block trend: {trend_text:?}"
    );

    // A battery the history has never seen renders the dotted placeholder.
    let cold = battery_lines(
        &[BatteryInfo::new("BAT9", Default::default())],
        history,
        TuiTheme::default(),
        60,
    );
    let cold_trend: String = cold[1]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(
        cold_trend.contains('·'),
        "unknown battery renders the placeholder: {cold_trend:?}"
    );
}
