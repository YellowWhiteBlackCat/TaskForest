//! Fan-page system thermal-zone render tests: the `power.thermal-zones` TUI
//! traversal surface, its honest dash for an unread zone, and its whole-group
//! admission. Split out of `device_render.rs` to respect the test source line
//! budget.

use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::SystemSnapshot;
use taskmanager_core::core::sensors::{
    SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
    SensorReading, SensorScale,
};
use taskmanager_shell::presentation::missing_value;

use super::frame_text;

/// A typed temperature reading in the wire shape the sensor-center provider
/// publishes: an observed value or an explicit typed failure — never a
/// fabricated zero.
fn thermal_zone_reading(
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

/// A typed fan channel for the same-device context of the thermal-zone tests.
fn fan_zone_reading(device_id: &str, channel: &str, label: &str, rpm: u64) -> SensorReading {
    SensorReading::from_measurement_observation(
        DeviceId::new(device_id),
        channel.into(),
        label.into(),
        SensorMeasurementObservation::available(
            SensorDescriptor::fan_speed(SensorScale::IDENTITY),
            SensorMagnitude::Unsigned(rpm),
            1_000,
        )
        .expect("valid fan fixture"),
    )
}

/// `power.thermal-zones` TUI delivery: the Fan page carries the SYSTEM thermal
/// surface — exactly one row per `Temperature` reading in the shared sensor
/// center, each named by the reading's own source label, with the shared `°C`
/// spelling for an observed value and the shared dash for an unread zone. The
/// same frame keeps the fan's device-level rows, so the traversal is proven to
/// reach a foreign-device zone the device-level `device_id` filter cannot.
#[test]
fn fan_panel_traverses_every_thermal_zone_reading_and_names_its_source() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Fan;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(SensorCenterSnapshot {
            state: DeviceState::healthy(1_000),
            timestamp_ms: 1_000,
            readings: vec![
                fan_zone_reading("hwmon:cpu", "cpu_fan", "cpu_fan", 2_400),
                thermal_zone_reading("hwmon:cpu", "cpu_package", "Package", Some(51.0)),
                thermal_zone_reading("thermal:acpitz", "acpitz:zone0", "acpitz", Some(61.0)),
                thermal_zone_reading("thermal:nvme0", "nvme0:zone0", "nvme0", None),
            ],
            ..Default::default()
        })),
    );

    let text = frame_text(&app, 140, 48);
    let unread = format!("nvme0 · {}", missing_value());

    // The fan's DEVICE-level context stays: its own channel rows plus the
    // same-device temperature row.
    assert!(text.contains("2400 RPM"), "fan speed must render:\n{text}");
    assert!(
        text.contains("Temperature Package · 51.0 °C"),
        "the device-level temperature row must stay:\n{text}"
    );
    // The SYSTEM traversal: the foreign-device zone is named by its own source
    // label, and the unread zone keeps its named row with the shared dash
    // instead of vanishing or fabricating a temperature.
    assert!(
        text.contains("acpitz · 61.0 °C"),
        "a foreign-device thermal zone must be named by its own label:\n{text}"
    );
    assert!(
        text.contains(&unread),
        "an unread zone must keep its named dash:\n{text}"
    );
    assert!(
        !text.contains("0.0 °C"),
        "an unread zone must never render a fabricated temperature:\n{text}"
    );
}

/// The Fan token is also the TUI's thermal surface: temperature readings alone
/// (the real Linux `thermal:*` case, where zones carry no fan channel) must
/// back the resource, so the selector reaches the system group on a fanless
/// host — and the panel still states the fan absence honestly.
#[test]
fn fan_resource_stays_reachable_and_honest_from_thermal_zones_alone() {
    let mut app = crate::TuiApp::from_shell(taskmanager_shell::ShellApp::new());
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(
            SystemSnapshot::default(),
        ))),
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(SensorCenterSnapshot {
            state: DeviceState::healthy(1_000),
            timestamp_ms: 1_000,
            readings: vec![
                thermal_zone_reading("thermal:acpitz", "acpitz:zone0", "acpitz", Some(61.0)),
                thermal_zone_reading("thermal:nvme0", "nvme0:zone0", "nvme0", None),
            ],
            ..Default::default()
        })),
    );

    let fan = crate::PerfDevice::Fan;
    assert!(
        app.visible_perf_devices().contains(&fan),
        "temperature readings alone must back the Fan/thermal resource"
    );
    assert_eq!(
        app.select_perf_device_digit('3'),
        Some(fan),
        "the digit selector must reach the thermal surface without any fan channel"
    );
    app.select_perf_device(fan);

    let text = frame_text(&app, 140, 48);
    assert!(
        text.contains("No fan sensor was detected"),
        "the fan group must state its absence honestly:\n{text}"
    );
    assert!(
        text.contains("acpitz · 61.0 °C"),
        "the system thermal group must paint without a fan channel:\n{text}"
    );
    assert!(
        text.contains(&format!("nvme0 · {}", missing_value())),
        "the unread zone must keep its named dash:\n{text}"
    );
}

/// The optional system group is admitted only as a WHOLE group: on a fanless
/// host, sweeping the terminal height from the supported floor to the capture
/// size must never paint a partial group (a single zone label appears only when
/// every zone row fits), the group never disappears again once admitted, and
/// the honest fan-absence line stays in every frame.
#[test]
fn fan_panel_admits_the_system_thermal_group_only_as_a_whole_group() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Fan;
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(SensorCenterSnapshot {
            state: DeviceState::healthy(1_000),
            timestamp_ms: 1_000,
            readings: vec![
                thermal_zone_reading("thermal:acpitz", "acpitz:zone0", "acpitz", Some(61.0)),
                thermal_zone_reading(
                    "thermal:x86_pkg_temp",
                    "pkg:zone0",
                    "x86_pkg_temp",
                    Some(71.0),
                ),
                thermal_zone_reading("thermal:nvme0", "nvme0:zone0", "nvme0", None),
            ],
            ..Default::default()
        })),
    );

    let zones = ["acpitz", "x86_pkg_temp", "nvme0"];
    let mut admitted_at: Option<u16> = None;
    for height in 16..=36 {
        let text = frame_text(&app, 54, height);
        let painted = zones.iter().filter(|label| text.contains(**label)).count();
        assert!(
            painted == 0 || painted == zones.len(),
            "height {height} painted a partial thermal group ({painted}/{}):\n{text}",
            zones.len()
        );
        assert!(
            text.contains("No fan sensor was detected"),
            "height {height} must keep the honest fan absence:\n{text}"
        );
        match admitted_at {
            None if painted == zones.len() => admitted_at = Some(height),
            Some(first) => assert_eq!(
                painted,
                zones.len(),
                "the group admitted at height {first} must stay admitted at {height}:\n{text}"
            ),
            None => {}
        }
    }
    assert!(
        admitted_at.is_some(),
        "the whole thermal group must fit at some supported terminal height"
    );

    let capture = frame_text(&app, 120, 36);
    for label in zones {
        assert!(
            capture.contains(label),
            "the capture frame must paint every named zone, missing {label:?}:\n{capture}"
        );
    }
}

/// The fan capture scene (`TM_TUI_CAPTURE_DEVICE=fan`) seeds named readable
/// zones plus an unread zone, so a real terminal frame proves the traversal
/// instead of a placeholder or a fabricated zero; it mirrors iced's capture
/// fixture zones.
#[test]
fn fan_capture_fixture_paints_named_thermal_zones_and_an_unread_dash() {
    let mut app = crate::demo_app();
    app.perf_device = crate::PerfDevice::Fan;
    crate::demo::seed_fan_capture_sensors(&mut app);

    let text = frame_text(&app, 120, 36);
    for expected in [
        "cpu_fan",
        "2400 RPM",
        "Temperature Package · 51.0 °C",
        "acpitz · 61.0 °C",
    ] {
        assert!(
            text.contains(expected),
            "the fan capture frame lost {expected:?}:\n{text}"
        );
    }
    assert!(
        text.contains(&format!("nvme0 · {}", missing_value())),
        "the capture fixture keeps an unread zone as a named dash:\n{text}"
    );
    assert!(
        !text.contains("0.0 °C"),
        "the capture fixture must never paint a fabricated temperature:\n{text}"
    );
}
