use super::*;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::sensors::{
    SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
    SensorScale,
};
use taskmanager_shell::presentation::missing_value;

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

/// Rendered text of one ratatui line (its spans concatenated).
fn rendered(line: &ratatui::text::Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// A temperature channel in exactly the wire shape the sensor-center provider
/// publishes: `Some` is a read value, `None` an explicit typed failure (never a
/// zero), with the reading's own device and source label.
fn zone_reading(
    device_id: &str,
    channel: &str,
    label: &str,
    temperature_c: Option<f64>,
) -> SensorReading {
    let descriptor = SensorDescriptor::temperature(SensorScale::IDENTITY);
    let observation = match temperature_c {
        Some(value) => SensorMeasurementObservation::available(
            descriptor,
            SensorMagnitude::Decimal(value),
            1_000,
        )
        .expect("valid thermal-zone fixture"),
        None => SensorMeasurementObservation::unavailable(
            descriptor,
            taskmanager_core::core::failure::FailureKind::PermissionDenied,
        ),
    };
    SensorReading::from_measurement_observation(
        DeviceId::new(device_id),
        channel.into(),
        label.into(),
        observation,
    )
}

/// The system thermal-zone group traverses the WHOLE shared reading list — one
/// row per `Temperature` reading, in projection order, each named by the
/// reading's own source label — and not just the selected fan's device. The fan
/// channel stays out of the group, the observed zone renders the shared `°C`
/// spelling, and an unread zone keeps its named row with the shared dash
/// instead of a fabricated `0.0 °C`.
#[test]
fn thermal_zone_lines_traverse_every_temperature_reading_and_name_its_source() {
    let sensors = SensorCenterSnapshot {
        readings: vec![
            zone_reading("thermal:acpitz", "acpitz:zone0", "acpitz", Some(61.0)),
            fan_reading("CPU Fan", "hwmon:cpu", 1_500),
            zone_reading(
                "thermal:x86_pkg_temp",
                "pkg:zone0",
                "x86_pkg_temp",
                Some(71.0),
            ),
            zone_reading("thermal:nvme0", "nvme0:zone0", "nvme0", None),
        ],
        ..SensorCenterSnapshot::default()
    };

    let lines = thermal_zone_lines(&sensors, TuiTheme::default());
    let text: Vec<String> = lines.iter().map(rendered).collect();

    assert_eq!(
        text[0],
        taskmanager_application::i18n::t("common.temperature"),
        "the group is headed by the shared temperature label: {text:?}"
    );
    assert_eq!(
        &text[1..],
        [
            "  acpitz · 61.0 °C",
            "  x86_pkg_temp · 71.0 °C",
            format!("  nvme0 · {}", missing_value()).as_str(),
        ],
        "one named row per temperature reading, in projection order, with the \
         shared dash for the unread zone: {text:?}"
    );
}

/// The device-level fan block filters by the fan's own `device_id`; the system
/// group traverses foreign-device zones the fan block cannot reach and keeps
/// the unread zone the fan block drops. This is the TUI distinction between the
/// fan's device context and the system thermal surface over the same facts.
#[test]
fn system_thermal_group_reaches_foreign_devices_the_fan_rows_cannot() {
    // The assertions below pin the English catalog words and the `·`
    // separators, so this test must hold the language guard and pin English:
    // another test's `zh` window must not leak in (the row would then fail for
    // a reason that has nothing to do with the device traversal it proves).
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);

    let sensors = SensorCenterSnapshot {
        readings: vec![
            fan_reading("CPU Fan", "hwmon:cpu", 1_500),
            zone_reading("hwmon:cpu", "cpu_package", "Package", Some(51.0)),
            zone_reading("thermal:acpitz", "acpitz:zone0", "acpitz", Some(61.0)),
            zone_reading("thermal:nvme0", "nvme0:zone0", "nvme0", None),
        ],
        ..SensorCenterSnapshot::default()
    };
    let shell = taskmanager_shell::ShellApp::new();

    let device_rows: Vec<String> = fan_lines(&sensors, &shell, TuiTheme::default(), 60)
        .iter()
        .map(rendered)
        .collect();
    assert!(
        device_rows
            .iter()
            .any(|row| row.contains("Temperature Package · 51.0 °C")),
        "the fan block keeps its same-device temperature row: {device_rows:?}"
    );
    assert!(
        !device_rows
            .iter()
            .any(|row| row.contains("acpitz") || row.contains("nvme0")),
        "the device-level rows must not leak foreign-device zones: {device_rows:?}"
    );

    let system_rows: Vec<String> = thermal_zone_lines(&sensors, TuiTheme::default())
        .iter()
        .map(rendered)
        .collect();
    let expected_dash = format!("  nvme0 · {}", missing_value());
    for expected in [
        "  Package · 51.0 °C",
        "  acpitz · 61.0 °C",
        expected_dash.as_str(),
    ] {
        assert!(
            system_rows.iter().any(|row| row == expected),
            "the system group must paint {expected:?}: {system_rows:?}"
        );
    }
    assert!(
        system_rows.iter().all(|row| !row.contains("0.0 °C")),
        "an unread zone must never fabricate a temperature: {system_rows:?}"
    );
}
