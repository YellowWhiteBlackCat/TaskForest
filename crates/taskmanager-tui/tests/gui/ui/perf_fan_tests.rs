use super::*;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::sensors::{
    SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
    SensorScale,
};

/// Build a fan `SensorReading` from its canonical measurement observation.
fn fan_reading(label: &str, device_id: &str, rpm: u32) -> SensorReading {
    SensorReading::from_measurement_observation(
        DeviceId::new(device_id),
        format!("{device_id}:{label}"),
        label.into(),
        SensorMeasurementObservation::available(
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(u64::from(rpm)),
            10,
        )
        .expect("valid fan fixture"),
    )
}

/// A fan whose RPM history has >=2 samples renders a real sparkline (a ramp
/// block) on its trend line; a fan with no history renders the dotted
/// placeholder. The trend line sits right after the header (index 1).
#[test]
fn fan_trend_line_matches_that_fans_own_history_window() {
    // Record two sensor snapshots for "CPU Fan"/"hwmon1" so its window has
    // >=2 samples. Fans live on the sensor event, not the system snapshot.
    let mut shell = taskmanager_shell::ShellApp::new();
    let snapshot = SensorCenterSnapshot {
        timestamp_ms: 10,
        readings: vec![fan_reading("CPU Fan", "hwmon1", 1500)],
        ..SensorCenterSnapshot::default()
    };
    let system = taskmanager_core::core::metrics::SystemSnapshot {
        timestamp_ms: snapshot.timestamp_ms,
        ..Default::default()
    };
    taskmanager_shell::fixture::record_demo_history_frame(
        &mut shell,
        &system,
        None,
        Some(&snapshot),
    );
    taskmanager_shell::fixture::record_demo_history_frame(
        &mut shell,
        &system,
        None,
        Some(&snapshot),
    );
    let history = &shell.history;
    // A constant RPM window resolves and trends to a flat mid-ramp.
    let window = history.fan_rpm_for("hwmon1:CPU Fan");
    assert_eq!(window.len(), 2, "two sensor snapshots recorded");
    assert!(
        super::super::sparkline::test_support::device_trend(&window).contains('▅'),
        "constant RPM → flat mid-ramp"
    );

    // The trend line in fan_lines is line index 1 (right after the header).
    let known = fan_lines(&snapshot, &shell, TuiTheme::default(), 60);
    let trend_text: String = known[1]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(
        trend_text.contains('▅'),
        "known fan renders a ramp-block trend: {trend_text:?}"
    );

    // A fan the history has never seen renders the dotted placeholder.
    let cold = SensorCenterSnapshot {
        timestamp_ms: 10,
        readings: vec![fan_reading("Case Fan", "hwmon9", 800)],
        ..SensorCenterSnapshot::default()
    };
    let cold_lines = fan_lines(&cold, &shell, TuiTheme::default(), 60);
    let cold_trend: String = cold_lines[1]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(
        cold_trend.contains('·'),
        "unknown fan renders the placeholder: {cold_trend:?}"
    );
}
